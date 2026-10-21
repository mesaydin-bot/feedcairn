//! Concurrent HTTP fetch worker. Parsing is feed-rs; persistence is ingest + SQLite.

use crate::app::AppState;
use crate::db;
use crate::error::Result;
use crate::models::{Feed, RefreshAllResult, RefreshResult};
use std::sync::Arc;
use tokio::sync::Semaphore;

const CONCURRENCY: usize = 6;

pub async fn refresh_many(state: &AppState, folder_id: Option<i64>) -> Result<RefreshAllResult> {
    let feeds: Vec<Feed> = state.with_db(|conn| {
        let all = db::list_feeds(conn)?;
        if let Some(fid) = folder_id {
            let ids = db::descendant_folder_ids(conn, fid)?;
            Ok(all
                .into_iter()
                .filter(|f| f.folder_id.map(|id| ids.contains(&id)).unwrap_or(false))
                .collect())
        } else {
            Ok(all)
        }
    })?;

    let sem = Arc::new(Semaphore::new(CONCURRENCY));
    let mut joins = Vec::with_capacity(feeds.len());
    for feed in feeds {
        let permit = sem
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");
        let state = state.clone();
        let id = feed.id;
        joins.push(tokio::spawn(async move {
            let _permit = permit;
            state.refresh_feed(id).await
        }));
    }

    let mut results = Vec::new();
    let mut feeds_ok = 0u32;
    let mut feeds_failed = 0u32;
    for join in joins {
        match join.await {
            Ok(Ok(res)) => {
                if res.error.is_some() {
                    feeds_failed += 1;
                } else {
                    feeds_ok += 1;
                }
                results.push(res);
            }
            Ok(Err(e)) => {
                feeds_failed += 1;
                results.push(RefreshResult {
                    feed_id: 0,
                    inserted: 0,
                    updated: 0,
                    skipped: 0,
                    not_modified: false,
                    error: Some(e.to_string()),
                });
            }
            Err(e) => {
                feeds_failed += 1;
                results.push(RefreshResult {
                    feed_id: 0,
                    inserted: 0,
                    updated: 0,
                    skipped: 0,
                    not_modified: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }
    Ok(RefreshAllResult {
        results,
        feeds_ok,
        feeds_failed,
    })
}
