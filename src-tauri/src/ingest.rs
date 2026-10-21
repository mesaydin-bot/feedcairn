use crate::db;
use crate::error::Result;
use crate::models::RefreshResult;
use feed_rs::model::{Entry, Feed, Text};
use rusqlite::Connection;

pub fn ingest_parsed(conn: &Connection, feed_id: i64, parsed: &Feed) -> Result<(u32, u32, u32)> {
    let fetched_at = db::now_unix();
    let mut inserted = 0u32;
    let mut updated = 0u32;
    let mut skipped = 0u32;

    if let Some(title) = parsed.title.as_ref().and_then(|t| nonempty(&t.content)) {
        let site = parsed
            .links
            .iter()
            .find(|l| l.rel.as_deref() == Some("alternate"))
            .or(parsed.links.first())
            .map(|l| l.href.as_str());
        let current = db::get_feed(conn, feed_id)?;
        if current.title == current.url || current.title.is_empty() {
            db::update_feed_identity(conn, feed_id, title, site)?;
        } else if current.site_url.is_none() {
            db::update_feed_identity(conn, feed_id, &current.title, site)?;
        }
    }

    for entry in &parsed.entries {
        match upsert_entry(conn, feed_id, entry, fetched_at)? {
            Upsert::Inserted => inserted += 1,
            Upsert::Updated => updated += 1,
            Upsert::Skipped => skipped += 1,
        }
    }
    Ok((inserted, updated, skipped))
}

enum Upsert {
    Inserted,
    Updated,
    Skipped,
}

fn upsert_entry(conn: &Connection, feed_id: i64, entry: &Entry, fetched_at: i64) -> Result<Upsert> {
    let url = entry
        .links
        .iter()
        .find(|l| l.rel.as_deref() == Some("alternate"))
        .or(entry.links.first())
        .map(|l| l.href.clone());
    let guid = guid_for(entry, url.as_deref());
    let title = entry
        .title
        .as_ref()
        .map(|t| t.content.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "(untitled)".to_string());
    let raw_html = entry_html(entry);
    let content_html = sanitize_html(&raw_html);
    let search_text = html2text::from_read(content_html.as_bytes(), 80).unwrap_or_default();
    let summary = entry
        .summary
        .as_ref()
        .map(|t| plain(t))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let t = search_text.trim();
            if t.chars().count() > 280 {
                format!("{}…", t.chars().take(280).collect::<String>())
            } else {
                t.to_string()
            }
        });
    let published = entry
        .published
        .or(entry.updated)
        .map(|d| d.timestamp());

    if let Some(id) = db::find_article_by_guid(conn, feed_id, &guid)? {
        db::update_article_content(
            conn,
            id,
            &title,
            url.as_deref(),
            published,
            &content_html,
            &summary,
            &search_text,
        )?;
        return Ok(Upsert::Updated);
    }
    if let Some(url) = url.as_deref() {
        if let Some(id) = db::find_article_by_url(conn, feed_id, url)? {
            db::update_article_content(
                conn,
                id,
                &title,
                Some(url),
                published,
                &content_html,
                &summary,
                &search_text,
            )?;
            return Ok(Upsert::Updated);
        }
    }

    db::insert_article(
        conn,
        feed_id,
        &guid,
        &title,
        url.as_deref(),
        published,
        &content_html,
        &summary,
        &search_text,
        fetched_at,
    )?;
    Ok(Upsert::Inserted)
}

pub fn guid_for(entry: &Entry, url: Option<&str>) -> String {
    let id = entry.id.trim();
    if !id.is_empty() {
        return id.to_string();
    }
    if let Some(url) = url.map(str::trim).filter(|s| !s.is_empty()) {
        return url.to_string();
    }
    let title = entry
        .title
        .as_ref()
        .map(|t| t.content.as_str())
        .unwrap_or("");
    let ts = entry
        .published
        .or(entry.updated)
        .map(|d| d.timestamp())
        .unwrap_or(0);
    format!("{title}|{ts}")
}

fn entry_html(entry: &Entry) -> String {
    if let Some(content) = &entry.content {
        if let Some(body) = &content.body {
            if !body.trim().is_empty() {
                return body.clone();
            }
        }
    }
    if let Some(summary) = &entry.summary {
        if !summary.content.trim().is_empty() {
            return if looks_like_html(&summary.content) {
                summary.content.clone()
            } else {
                format!("<p>{}</p>", html_escape(&summary.content))
            };
        }
    }
    String::new()
}

fn looks_like_html(s: &str) -> bool {
    let t = s.trim_start();
    t.starts_with('<')
}

fn plain(text: &Text) -> String {
    if looks_like_html(&text.content) {
        html2text::from_read(text.content.as_bytes(), 80)
            .unwrap_or_default()
            .trim()
            .to_string()
    } else {
        text.content.trim().to_string()
    }
}

fn nonempty(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn find_entry_html(parsed: &Feed, guid: &str, url: Option<&str>) -> Option<String> {
    for entry in &parsed.entries {
        let entry_url = entry
            .links
            .iter()
            .find(|l| l.rel.as_deref() == Some("alternate"))
            .or(entry.links.first())
            .map(|l| l.href.clone());
        let entry_guid = guid_for(entry, entry_url.as_deref());
        let guid_match = entry_guid == guid;
        let url_match = match (url, entry_url.as_deref()) {
            (Some(a), Some(b)) if !a.is_empty() && a == b => true,
            _ => false,
        };
        if guid_match || url_match {
            let html = sanitize_html(&entry_html(entry));
            if !html.trim().is_empty() {
                return Some(html);
            }
        }
    }
    None
}

/// Build safe HTML from a summary string (HTML or plain).
pub fn html_from_summary_text(summary: &str) -> String {
    let t = summary.trim();
    if t.is_empty() {
        return String::new();
    }
    if looks_like_html(t) {
        sanitize_html(t)
    } else {
        sanitize_html(&format!("<p>{}</p>", html_escape(t)))
    }
}

pub fn sanitize_html(html: &str) -> String {
    ammonia::Builder::default()
        .link_rel(Some("noopener noreferrer"))
        .clean(html)
        .to_string()
}

pub fn empty_refresh(feed_id: i64, not_modified: bool, error: Option<String>) -> RefreshResult {
    RefreshResult {
        feed_id,
        inserted: 0,
        updated: 0,
        skipped: 0,
        not_modified,
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use feed_rs::parser;

    const RSS: &str = r#"<?xml version="1.0"?>
<rss version="2.0">
  <channel>
    <title>Example Feed</title>
    <link>https://example.com/</link>
    <item>
      <title>First post</title>
      <link>https://example.com/one</link>
      <guid>https://example.com/one</guid>
      <description>Hello &lt;b&gt;world&lt;/b&gt;</description>
    </item>
    <item>
      <title>Second post</title>
      <link>https://example.com/two</link>
      <guid>urn:uuid:1234</guid>
      <description>More news</description>
    </item>
  </channel>
</rss>"#;

    const RSS_DUP_URL: &str = r#"<?xml version="1.0"?>
<rss version="2.0">
  <channel>
    <title>Example Feed</title>
    <item>
      <title>First post (updated)</title>
      <link>https://example.com/one</link>
      <guid>https://example.com/one-new-guid</guid>
      <description>Updated body</description>
    </item>
    <item>
      <title>First post</title>
      <link>https://example.com/one</link>
      <guid>https://example.com/one</guid>
      <description>Hello again</description>
    </item>
  </channel>
</rss>"#;

    #[test]
    fn ingest_inserts_then_dedupes_by_guid() {
        let conn = db::open_memory().unwrap();
        let (feed, _) = db::add_feed(&conn, "https://example.com/rss.xml", "Example", None, None).unwrap();
        let parsed = parser::parse(RSS.as_bytes()).unwrap();
        let (ins, upd, skip) = ingest_parsed(&conn, feed.id, &parsed).unwrap();
        assert_eq!(ins, 2);
        assert_eq!(upd, 0);
        assert_eq!(skip, 0);

        let (ins2, upd2, skip2) = ingest_parsed(&conn, feed.id, &parsed).unwrap();
        assert_eq!(ins2, 0);
        assert_eq!(upd2, 2);
        assert_eq!(skip2, 0);

        let items = db::list_articles(&conn, &crate::models::ArticleFilter::default()).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn ingest_dedupes_by_url_when_guid_changes() {
        let conn = db::open_memory().unwrap();
        let (feed, _) = db::add_feed(&conn, "https://example.com/rss.xml", "Example", None, None).unwrap();
        let parsed = parser::parse(RSS.as_bytes()).unwrap();
        ingest_parsed(&conn, feed.id, &parsed).unwrap();
        let parsed2 = parser::parse(RSS_DUP_URL.as_bytes()).unwrap();
        let (ins, upd, _) = ingest_parsed(&conn, feed.id, &parsed2).unwrap();
        // one guid is new but URL matches an existing item → update, plus the original guid updates
        assert_eq!(ins, 0);
        assert!(upd >= 1);
        let items = db::list_articles(&conn, &crate::models::ArticleFilter::default()).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn sanitize_strips_script() {
        let dirty = r#"<p>Hi<script>alert(1)</script><a href="javascript:alert(1)">x</a></p>"#;
        let clean = sanitize_html(dirty);
        assert!(!clean.contains("script"));
        assert!(!clean.contains("javascript:"));
        assert!(clean.contains("<p>Hi"));
    }
}
