use crate::error::{Error, Result};
use crate::models::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::Path;

const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;

CREATE TABLE IF NOT EXISTS folders (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  parent_id INTEGER REFERENCES folders(id) ON DELETE CASCADE,
  sort INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS feeds (
  id INTEGER PRIMARY KEY,
  url TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL,
  site_url TEXT,
  folder_id INTEGER REFERENCES folders(id) ON DELETE SET NULL,
  etag TEXT,
  last_modified TEXT,
  last_fetched INTEGER,
  last_error TEXT
);

CREATE TABLE IF NOT EXISTS articles (
  id INTEGER PRIMARY KEY,
  feed_id INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
  guid TEXT NOT NULL,
  title TEXT NOT NULL,
  url TEXT,
  published INTEGER,
  content_html TEXT NOT NULL DEFAULT '',
  summary TEXT NOT NULL DEFAULT '',
  search_text TEXT NOT NULL DEFAULT '',
  is_read INTEGER NOT NULL DEFAULT 0,
  is_saved INTEGER NOT NULL DEFAULT 0,
  fetched_at INTEGER NOT NULL,
  UNIQUE(feed_id, guid)
);

CREATE INDEX IF NOT EXISTS idx_articles_feed_read ON articles(feed_id, is_read);
CREATE INDEX IF NOT EXISTS idx_articles_published ON articles(published DESC);
CREATE INDEX IF NOT EXISTS idx_articles_saved ON articles(is_saved);
CREATE INDEX IF NOT EXISTS idx_articles_url ON articles(feed_id, url);
CREATE INDEX IF NOT EXISTS idx_feeds_folder ON feeds(folder_id);

CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS notes (
  id INTEGER PRIMARY KEY,
  title TEXT NOT NULL,
  body_md TEXT NOT NULL DEFAULT '',
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(updated_at DESC);

CREATE TABLE IF NOT EXISTS prompt_templates (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  body TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_prompt_templates_updated ON prompt_templates(updated_at DESC);
"#;

const DEFAULT_SETTINGS: &[(&str, &str)] = &[
    ("theme", "dark"),
    ("ai_provider", "chatgpt"),
    ("ai_cmd_chatgpt", "codex"),
    ("ai_args_chatgpt", ""),
    ("ai_model_chatgpt", "gpt-6-astra"),
    ("ai_effort_chatgpt", "xhigh"),
    ("ai_cmd_kimi", "kimi"),
    ("ai_args_kimi", ""),
    ("ai_model_kimi", "kimi-code/kimi-for-coding"),
    ("ai_thinking_kimi", "true"),
    ("ai_cmd_cursor", "cursor"),
    ("ai_args_cursor", ""),
    ("ai_model_cursor", "auto"),
    ("ai_effort_cursor", "high"),
    ("ai_cmd_antigravity", "antigravity"),
    ("ai_args_antigravity", ""),
    ("ai_cmd_claude", "claude"),
    ("ai_args_claude", ""),
    ("ai_model_claude", "default"),
    ("ai_effort_claude", "default"),
    ("mark_read_on_open", "true"),
    ("unread_only_default", "true"),
];

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn open_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    for (k, v) in DEFAULT_SETTINGS {
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
            params![k, v],
        )?;
    }
    // Broken openai `--quiet` default must never stick around.
    conn.execute(
        "UPDATE settings SET value = '' WHERE key LIKE 'ai_args_%' AND trim(value) = '--quiet'",
        [],
    )?;
    // Prefer Codex for ChatGPT subscription when still on legacy openai default.
    if let Ok(Some(cmd)) = get_setting(conn, "ai_cmd_chatgpt") {
        if cmd.trim() == "openai" || cmd.trim() == "chatgpt" {
            let _ = set_setting(conn, "ai_cmd_chatgpt", "codex");
        }
    }
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |r| r.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub fn list_settings(conn: &Connection) -> Result<Vec<Setting>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
    let rows = stmt.query_map([], |r| {
        Ok(Setting {
            key: r.get(0)?,
            value: r.get(1)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn counts(conn: &Connection) -> Result<Counts> {
    Ok(Counts {
        unread: conn.query_row(
            "SELECT COUNT(*) FROM articles WHERE is_read = 0",
            [],
            |r| r.get(0),
        )?,
        saved: conn.query_row(
            "SELECT COUNT(*) FROM articles WHERE is_saved = 1",
            [],
            |r| r.get(0),
        )?,
        all: conn.query_row("SELECT COUNT(*) FROM articles", [], |r| r.get(0))?,
    })
}

fn folder_from_row(r: &rusqlite::Row) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: r.get(0)?,
        name: r.get(1)?,
        parent_id: r.get(2)?,
        sort: r.get(3)?,
        unread: 0,
    })
}

fn feed_from_row(r: &rusqlite::Row) -> rusqlite::Result<Feed> {
    Ok(Feed {
        id: r.get(0)?,
        url: r.get(1)?,
        title: r.get(2)?,
        site_url: r.get(3)?,
        folder_id: r.get(4)?,
        etag: r.get(5)?,
        last_modified: r.get(6)?,
        last_fetched: r.get(7)?,
        last_error: r.get(8)?,
        unread: r.get(9)?,
    })
}

pub fn list_folders(conn: &Connection) -> Result<Vec<Folder>> {
    let mut stmt =
        conn.prepare("SELECT id, name, parent_id, sort FROM folders ORDER BY sort, name")?;
    let rows = stmt.query_map([], folder_from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn list_feeds(conn: &Connection) -> Result<Vec<Feed>> {
    let mut stmt = conn.prepare(
        "SELECT f.id, f.url, f.title, f.site_url, f.folder_id, f.etag, f.last_modified,
                f.last_fetched, f.last_error,
                (SELECT COUNT(*) FROM articles a WHERE a.feed_id = f.id AND a.is_read = 0) AS unread
         FROM feeds f
         ORDER BY f.title COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], feed_from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_feed(conn: &Connection, id: i64) -> Result<Feed> {
    conn.query_row(
        "SELECT f.id, f.url, f.title, f.site_url, f.folder_id, f.etag, f.last_modified,
                f.last_fetched, f.last_error,
                (SELECT COUNT(*) FROM articles a WHERE a.feed_id = f.id AND a.is_read = 0) AS unread
         FROM feeds f WHERE f.id = ?1",
        params![id],
        feed_from_row,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::msg("Feed not found"),
        other => other.into(),
    })
}

pub fn create_folder(conn: &Connection, name: &str, parent_id: Option<i64>) -> Result<Folder> {
    let sort: i64 = conn.query_row(
        "SELECT COALESCE(MAX(sort), -1) + 1 FROM folders WHERE parent_id IS ?1",
        params![parent_id],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO folders (name, parent_id, sort) VALUES (?1, ?2, ?3)",
        params![name, parent_id, sort],
    )?;
    let id = conn.last_insert_rowid();
    Ok(Folder {
        id,
        name: name.to_string(),
        parent_id,
        sort,
        unread: 0,
    })
}

pub fn rename_folder(conn: &Connection, id: i64, name: &str) -> Result<()> {
    let n = conn.execute(
        "UPDATE folders SET name = ?1 WHERE id = ?2",
        params![name, id],
    )?;
    if n == 0 {
        return Err(Error::msg("Folder not found"));
    }
    Ok(())
}

pub fn delete_folder(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM folders WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn find_or_create_folder(
    conn: &Connection,
    name: &str,
    parent_id: Option<i64>,
) -> Result<i64> {
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM folders WHERE name = ?1 AND parent_id IS ?2",
            params![name, parent_id],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
    {
        return Ok(id);
    }
    Ok(create_folder(conn, name, parent_id)?.id)
}

pub fn add_feed(
    conn: &Connection,
    url: &str,
    title: &str,
    site_url: Option<&str>,
    folder_id: Option<i64>,
) -> Result<(Feed, bool)> {
    if let Some(existing) = conn
        .query_row(
            "SELECT f.id, f.url, f.title, f.site_url, f.folder_id, f.etag, f.last_modified,
                    f.last_fetched, f.last_error, 0
             FROM feeds f WHERE f.url = ?1",
            params![url],
            feed_from_row,
        )
        .optional()?
    {
        return Ok((existing, false));
    }
    conn.execute(
        "INSERT INTO feeds (url, title, site_url, folder_id) VALUES (?1, ?2, ?3, ?4)",
        params![url, title, site_url, folder_id],
    )?;
    let id = conn.last_insert_rowid();
    Ok((get_feed(conn, id)?, true))
}

pub fn update_feed(
    conn: &Connection,
    id: i64,
    title: Option<&str>,
    folder_id: Option<Option<i64>>,
) -> Result<Feed> {
    if let Some(title) = title {
        conn.execute(
            "UPDATE feeds SET title = ?1 WHERE id = ?2",
            params![title, id],
        )?;
    }
    if let Some(folder_id) = folder_id {
        conn.execute(
            "UPDATE feeds SET folder_id = ?1 WHERE id = ?2",
            params![folder_id, id],
        )?;
    }
    get_feed(conn, id)
}

pub fn delete_feed(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM feeds WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn update_fetch_meta(
    conn: &Connection,
    id: i64,
    etag: Option<&str>,
    last_modified: Option<&str>,
    last_fetched: i64,
    last_error: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE feeds SET etag = COALESCE(?1, etag), last_modified = COALESCE(?2, last_modified),
                last_fetched = ?3, last_error = ?4
         WHERE id = ?5",
        params![etag, last_modified, last_fetched, last_error, id],
    )?;
    Ok(())
}

pub fn update_feed_identity(
    conn: &Connection,
    id: i64,
    title: &str,
    site_url: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE feeds SET title = ?1, site_url = COALESCE(?2, site_url) WHERE id = ?3",
        params![title, site_url, id],
    )?;
    Ok(())
}

pub fn find_article_by_guid(conn: &Connection, feed_id: i64, guid: &str) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM articles WHERE feed_id = ?1 AND guid = ?2",
        params![feed_id, guid],
        |r| r.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub fn find_article_by_url(conn: &Connection, feed_id: i64, url: &str) -> Result<Option<i64>> {
    if url.is_empty() {
        return Ok(None);
    }
    conn.query_row(
        "SELECT id FROM articles WHERE feed_id = ?1 AND url = ?2",
        params![feed_id, url],
        |r| r.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub fn insert_article(
    conn: &Connection,
    feed_id: i64,
    guid: &str,
    title: &str,
    url: Option<&str>,
    published: Option<i64>,
    content_html: &str,
    summary: &str,
    search_text: &str,
    fetched_at: i64,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO articles (feed_id, guid, title, url, published, content_html, summary, search_text, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            feed_id,
            guid,
            title,
            url,
            published,
            content_html,
            summary,
            search_text,
            fetched_at
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_article_content(
    conn: &Connection,
    id: i64,
    title: &str,
    url: Option<&str>,
    published: Option<i64>,
    content_html: &str,
    summary: &str,
    search_text: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE articles SET title = ?1, url = ?2, published = COALESCE(?3, published),
                content_html = ?4, summary = ?5, search_text = ?6
         WHERE id = ?7",
        params![title, url, published, content_html, summary, search_text, id],
    )?;
    Ok(())
}

pub fn update_article_full_html(
    conn: &Connection,
    id: i64,
    content_html: &str,
    search_text: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE articles SET content_html = ?1, search_text = ?2 WHERE id = ?3",
        params![content_html, search_text, id],
    )?;
    Ok(())
}

pub fn get_article(conn: &Connection, id: i64) -> Result<Article> {
    conn.query_row(
        "SELECT a.id, a.feed_id, f.title, f.folder_id, a.guid, a.title, a.url, a.published,
                a.content_html, a.summary, a.is_read, a.is_saved, a.fetched_at
         FROM articles a JOIN feeds f ON f.id = a.feed_id
         WHERE a.id = ?1",
        params![id],
        |r| {
            Ok(Article {
                id: r.get(0)?,
                feed_id: r.get(1)?,
                feed_title: r.get(2)?,
                folder_id: r.get(3)?,
                guid: r.get(4)?,
                title: r.get(5)?,
                url: r.get(6)?,
                published: r.get(7)?,
                content_html: r.get(8)?,
                summary: r.get(9)?,
                is_read: r.get::<_, i64>(10)? != 0,
                is_saved: r.get::<_, i64>(11)? != 0,
                fetched_at: r.get(12)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::msg("Article not found"),
        other => other.into(),
    })
}

pub fn list_articles(conn: &Connection, filter: &ArticleFilter) -> Result<Vec<ArticleListItem>> {
    let mut sql = String::from(
        "SELECT a.id, a.feed_id, f.title, a.title, a.url, a.published, a.summary, a.is_read, a.is_saved
         FROM articles a
         JOIN feeds f ON f.id = a.feed_id
         WHERE 1=1",
    );
    let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(feed_id) = filter.feed_id {
        sql.push_str(" AND a.feed_id = ?");
        params_vec.push(Box::new(feed_id));
    }
    if let Some(folder_id) = filter.folder_id {
        let ids = descendant_folder_ids(conn, folder_id)?;
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        sql.push_str(&format!(" AND f.folder_id IN ({placeholders})"));
        for id in ids {
            params_vec.push(Box::new(id));
        }
    }
    if filter.saved_only {
        sql.push_str(" AND a.is_saved = 1");
    }
    if filter.unread_only {
        sql.push_str(" AND a.is_read = 0");
    }
    if let Some(q) = filter.query.as_ref().map(|s| s.trim().to_string()) {
        if !q.is_empty() {
            sql.push_str(" AND (a.title LIKE ? OR a.search_text LIKE ? OR f.title LIKE ?)");
            let like = format!("%{q}%");
            params_vec.push(Box::new(like.clone()));
            params_vec.push(Box::new(like.clone()));
            params_vec.push(Box::new(like));
        }
    }
    sql.push_str(" ORDER BY COALESCE(a.published, a.fetched_at) DESC, a.id DESC");
    let limit = filter.limit.unwrap_or(200).clamp(1, 500);
    let offset = filter.offset.unwrap_or(0).max(0);
    sql.push_str(" LIMIT ? OFFSET ?");
    params_vec.push(Box::new(limit));
    params_vec.push(Box::new(offset));

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(param_refs.as_slice(), |r| {
        Ok(ArticleListItem {
            id: r.get(0)?,
            feed_id: r.get(1)?,
            feed_title: r.get(2)?,
            title: r.get(3)?,
            url: r.get(4)?,
            published: r.get(5)?,
            summary: r.get(6)?,
            is_read: r.get::<_, i64>(7)? != 0,
            is_saved: r.get::<_, i64>(8)? != 0,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn set_read(conn: &Connection, id: i64, is_read: bool) -> Result<Article> {
    conn.execute(
        "UPDATE articles SET is_read = ?1 WHERE id = ?2",
        params![is_read as i64, id],
    )?;
    get_article(conn, id)
}

pub fn set_saved(conn: &Connection, id: i64, is_saved: bool) -> Result<Article> {
    conn.execute(
        "UPDATE articles SET is_saved = ?1 WHERE id = ?2",
        params![is_saved as i64, id],
    )?;
    get_article(conn, id)
}

pub fn mark_all_read(conn: &Connection, filter: &ArticleFilter) -> Result<u64> {
    let mut sql = String::from(
        "UPDATE articles SET is_read = 1 WHERE is_read = 0 AND id IN (
            SELECT a.id FROM articles a JOIN feeds f ON f.id = a.feed_id WHERE 1=1",
    );
    let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    if let Some(feed_id) = filter.feed_id {
        sql.push_str(" AND a.feed_id = ?");
        params_vec.push(Box::new(feed_id));
    }
    if let Some(folder_id) = filter.folder_id {
        let ids = descendant_folder_ids(conn, folder_id)?;
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        sql.push_str(&format!(" AND f.folder_id IN ({placeholders})"));
        for id in ids {
            params_vec.push(Box::new(id));
        }
    }
    if filter.saved_only {
        sql.push_str(" AND a.is_saved = 1");
    }
    if let Some(q) = filter.query.as_ref().map(|s| s.trim().to_string()) {
        if !q.is_empty() {
            sql.push_str(" AND (a.title LIKE ? OR a.search_text LIKE ?)");
            let like = format!("%{q}%");
            params_vec.push(Box::new(like.clone()));
            params_vec.push(Box::new(like));
        }
    }
    sql.push(')');
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let n = conn.execute(&sql, param_refs.as_slice())?;
    Ok(n as u64)
}

pub fn unread_titles_in_folder(conn: &Connection, folder_id: i64, limit: i64) -> Result<Vec<ArticleListItem>> {
    list_articles(
        conn,
        &ArticleFilter {
            folder_id: Some(folder_id),
            unread_only: true,
            limit: Some(limit),
            ..Default::default()
        },
    )
}

pub fn descendant_folder_ids(conn: &Connection, root: i64) -> Result<Vec<i64>> {
    let folders = list_folders(conn)?;
    let mut children: HashMap<Option<i64>, Vec<i64>> = HashMap::new();
    for f in &folders {
        children.entry(f.parent_id).or_default().push(f.id);
    }
    let mut out = vec![root];
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if let Some(kids) = children.get(&Some(id)) {
            for k in kids {
                out.push(*k);
                stack.push(*k);
            }
        }
    }
    Ok(out)
}

pub fn bootstrap(conn: &Connection) -> Result<Bootstrap> {
    let folders = list_folders(conn)?;
    let feeds = list_feeds(conn)?;
    let unread_by_folder = rollup_unread(&folders, &feeds);
    let folders: Vec<Folder> = folders
        .into_iter()
        .map(|mut f| {
            f.unread = *unread_by_folder.get(&f.id).unwrap_or(&0);
            f
        })
        .collect();
    let tree = build_tree(&folders, &feeds, None);
    let root_feeds = feeds
        .iter()
        .filter(|f| f.folder_id.is_none())
        .cloned()
        .collect();
    Ok(Bootstrap {
        folders: tree,
        root_feeds,
        counts: counts(conn)?,
        settings: list_settings(conn)?,
    })
}

fn rollup_unread(folders: &[Folder], feeds: &[Feed]) -> HashMap<i64, i64> {
    let mut direct: HashMap<i64, i64> = HashMap::new();
    for f in feeds {
        if let Some(fid) = f.folder_id {
            *direct.entry(fid).or_insert(0) += f.unread;
        }
    }
    let mut children: HashMap<Option<i64>, Vec<i64>> = HashMap::new();
    for f in folders {
        children.entry(f.parent_id).or_default().push(f.id);
    }
    fn dfs(
        id: i64,
        children: &HashMap<Option<i64>, Vec<i64>>,
        direct: &HashMap<i64, i64>,
        out: &mut HashMap<i64, i64>,
    ) -> i64 {
        let mut total = *direct.get(&id).unwrap_or(&0);
        if let Some(kids) = children.get(&Some(id)) {
            for k in kids {
                total += dfs(*k, children, direct, out);
            }
        }
        out.insert(id, total);
        total
    }
    let mut out = HashMap::new();
    if let Some(roots) = children.get(&None) {
        for r in roots {
            dfs(*r, &children, &direct, &mut out);
        }
    }
    // orphan folders with a missing parent still count
    for f in folders {
        out.entry(f.id).or_insert(*direct.get(&f.id).unwrap_or(&0));
    }
    out
}

fn build_tree(folders: &[Folder], feeds: &[Feed], parent: Option<i64>) -> Vec<FolderNode> {
    folders
        .iter()
        .filter(|f| f.parent_id == parent)
        .map(|f| FolderNode {
            folder: f.clone(),
            feeds: feeds
                .iter()
                .filter(|feed| feed.folder_id == Some(f.id))
                .cloned()
                .collect(),
            children: build_tree(folders, feeds, Some(f.id)),
        })
        .collect()
}


pub fn list_notes(conn: &Connection) -> Result<Vec<Note>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, body_md, updated_at FROM notes ORDER BY updated_at DESC, id DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Note {
            id: r.get(0)?,
            title: r.get(1)?,
            body_md: r.get(2)?,
            updated_at: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_note(conn: &Connection, id: i64) -> Result<Note> {
    conn.query_row(
        "SELECT id, title, body_md, updated_at FROM notes WHERE id = ?1",
        params![id],
        |r| {
            Ok(Note {
                id: r.get(0)?,
                title: r.get(1)?,
                body_md: r.get(2)?,
                updated_at: r.get(3)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::msg("Note not found"),
        other => other.into(),
    })
}

pub fn create_note(conn: &Connection, title: &str, body_md: &str) -> Result<Note> {
    let title = title.trim();
    let title = if title.is_empty() { "Untitled" } else { title };
    let now = now_unix();
    conn.execute(
        "INSERT INTO notes (title, body_md, updated_at) VALUES (?1, ?2, ?3)",
        params![title, body_md, now],
    )?;
    get_note(conn, conn.last_insert_rowid())
}

pub fn update_note(
    conn: &Connection,
    id: i64,
    title: Option<&str>,
    body_md: Option<&str>,
) -> Result<Note> {
    let existing = get_note(conn, id)?;
    let title = title
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(&existing.title);
    let body = body_md.unwrap_or(&existing.body_md);
    let now = now_unix();
    conn.execute(
        "UPDATE notes SET title = ?1, body_md = ?2, updated_at = ?3 WHERE id = ?4",
        params![title, body, now, id],
    )?;
    get_note(conn, id)
}

pub fn append_note(conn: &Connection, id: i64, chunk: &str) -> Result<Note> {
    let existing = get_note(conn, id)?;
    let body = if existing.body_md.trim().is_empty() {
        chunk.to_string()
    } else {
        format!("{}

{}", existing.body_md.trim_end(), chunk.trim())
    };
    update_note(conn, id, None, Some(&body))
}

pub fn delete_note(conn: &Connection, id: i64) -> Result<()> {
    let n = conn.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(Error::msg("Note not found"));
    }
    Ok(())
}


pub fn list_prompt_templates(conn: &Connection) -> Result<Vec<PromptTemplate>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, body, created_at, updated_at FROM prompt_templates ORDER BY updated_at DESC, id DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(PromptTemplate {
            id: r.get(0)?,
            name: r.get(1)?,
            body: r.get(2)?,
            created_at: r.get(3)?,
            updated_at: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_prompt_template(conn: &Connection, id: i64) -> Result<PromptTemplate> {
    conn.query_row(
        "SELECT id, name, body, created_at, updated_at FROM prompt_templates WHERE id = ?1",
        params![id],
        |r| {
            Ok(PromptTemplate {
                id: r.get(0)?,
                name: r.get(1)?,
                body: r.get(2)?,
                created_at: r.get(3)?,
                updated_at: r.get(4)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::msg("Prompt template not found"),
        other => other.into(),
    })
}

pub fn create_prompt_template(conn: &Connection, name: &str, body: &str) -> Result<PromptTemplate> {
    let name = name.trim();
    let name = if name.is_empty() { "Şablon" } else { name };
    let body = body.trim().trim_start_matches('/').trim();
    if body.is_empty() {
        return Err(Error::msg("Prompt template body is empty"));
    }
    let now = now_unix();
    conn.execute(
        "INSERT INTO prompt_templates (name, body, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![name, body, now, now],
    )?;
    get_prompt_template(conn, conn.last_insert_rowid())
}

pub fn update_prompt_template(
    conn: &Connection,
    id: i64,
    name: Option<&str>,
    body: Option<&str>,
) -> Result<PromptTemplate> {
    let existing = get_prompt_template(conn, id)?;
    let name = name
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(&existing.name);
    let body = match body {
        Some(b) => {
            let cleaned = b.trim().trim_start_matches('/').trim();
            if cleaned.is_empty() {
                return Err(Error::msg("Prompt template body is empty"));
            }
            cleaned.to_string()
        }
        None => existing.body.clone(),
    };
    let now = now_unix();
    conn.execute(
        "UPDATE prompt_templates SET name = ?1, body = ?2, updated_at = ?3 WHERE id = ?4",
        params![name, body, now, id],
    )?;
    get_prompt_template(conn, id)
}

pub fn delete_prompt_template(conn: &Connection, id: i64) -> Result<()> {
    let n = conn.execute("DELETE FROM prompt_templates WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(Error::msg("Prompt template not found"));
    }
    Ok(())
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_crud_roundtrip() {
        let conn = open_memory().unwrap();
        let n = create_note(&conn, "Hello", "# hi").unwrap();
        assert_eq!(n.title, "Hello");
        let listed = list_notes(&conn).unwrap();
        assert_eq!(listed.len(), 1);
        let updated = update_note(&conn, n.id, Some("Hello 2"), Some("body")).unwrap();
        assert_eq!(updated.title, "Hello 2");
        let appended = append_note(&conn, n.id, "more").unwrap();
        assert!(appended.body_md.contains("body"));
        assert!(appended.body_md.contains("more"));
        delete_note(&conn, n.id).unwrap();
        assert!(list_notes(&conn).unwrap().is_empty());
    }

    #[test]
    fn prompt_templates_crud_roundtrip() {
        let conn = open_memory().unwrap();
        let t = create_prompt_template(&conn, "Özet", "özetle").unwrap();
        assert_eq!(t.name, "Özet");
        assert_eq!(t.body, "özetle");
        let listed = list_prompt_templates(&conn).unwrap();
        assert_eq!(listed.len(), 1);
        let updated = update_prompt_template(&conn, t.id, Some("Tablo"), Some("/tablo")).unwrap();
        assert_eq!(updated.name, "Tablo");
        assert_eq!(updated.body, "tablo");
        delete_prompt_template(&conn, t.id).unwrap();
        assert!(list_prompt_templates(&conn).unwrap().is_empty());
    }

    #[test]
    fn migrate_clears_quiet_args() {
        let conn = open_memory().unwrap();
        set_setting(&conn, "ai_args_chatgpt", "--quiet").unwrap();
        migrate(&conn).unwrap();
        let v = get_setting(&conn, "ai_args_chatgpt").unwrap().unwrap();
        assert_eq!(v, "");
    }
}
