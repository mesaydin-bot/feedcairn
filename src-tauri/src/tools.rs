//! Allowlisted AI operator tools. Unknown tool names fail closed.

use crate::app::AppState;
use crate::db;
use crate::error::{Error, Result};
use crate::models::*;
use serde_json::Value;

const ALLOWED: &[&str] = &[
    "summarize_article",
    "analyze_feeds",
    "analyze_selection",
    "analyze_search",
    "suggest_feeds",
    "search_rss_web",
    "add_feed",
    "write_note",
    "list_notes",
];

pub fn tools_catalog() -> &'static str {
    r#"Allowed tools (emit ONLY these names; unknown tools are rejected):
- summarize_article: {"article_id": <number>}
- analyze_feeds: {"folder_id"?: <number>, "limit"?: <number>}
- analyze_selection: {}  // uses the provided UI context
- analyze_search: {"query": "<string>"}  // local SQLite only
- suggest_feeds: {"topic"?: "<string>"}  // already-subscribed catalog only — NOT internet search
- search_rss_web: {"query": "<string>", "months"?: <1-12>}  // real internet Google News RSS search
- add_feed: {"url": "<feed url>", "folder_id"?: <number>, "title"?: "<string>"}
- write_note: {"title"?: "<string>", "body_md": "<markdown>", "note_id"?: <number>, "append"?: true|false}
- list_notes: {}

To call tools, include a fenced JSON block:
```json
{"tools":[{"name":"write_note","args":{"title":"Brief","body_md":"- point"}}]}
```
You may also answer in plain prose with no tools.

Tool behavior:
- summarize_article / analyze_* return SQLite article data (ids, titles, urls, body excerpts). They do NOT run nested AI.
- search_rss_web fetches live Google News RSS for the query (last N months). MUST use this when the user asks for internet/RSS/web/Google News search, "internetten", "rss kaynak", "abone", or "son X ay" news discovery. Do NOT use suggest_feeds or analyze_search for those.
- write a clean Reader reply in prose, in the user's language, after tool JSON (or the host will run a second pass).

IMPORTANT — user-visible final answer:
- Follow the user's request. Do not force a fixed bullet summary, table shape, mermaid-vs-HTML choice, or visual style unless they asked for it.
- Reply in the user's language. Any format they ask for is fine (prose, GFM, HTML, SVG, mermaid, tables, …).
- After any tool JSON fence, write the Reader-visible answer. Never put tool JSON / ```json / {"tools":...} in that visible answer.
- For search_rss_web (when user did not specify another format): list each hit with title link, short summary, date/source. If empty, say clearly. Do NOT invent results. UI shows Subscribe buttons from FEED_URL lines.
- Tool protocol stays the same (fenced JSON above); the visible answer is separate."#
}

pub fn build_ask_prompt(user: &str, context_blob: &str, lang_rule: &str) -> String {
    let open_article_hint = if context_blob.contains("article_id:")
        && (context_blob.contains("excerpt:") || context_blob.contains("body:"))
    {
        "\n\nOPEN ARTICLE: Article title+body already in context — do not call summarize_article just to read it. Answer the user request directly. Call tools only if you need SQLite/actions."
    } else {
        ""
    };
    format!(
        "You are Feedcairn's local AI operator (personal RSS reader). Be concrete. Follow the user's request; do not impose a fixed answer shape or graph format. {lang_rule} Do not put tool JSON in the user-visible answer.{open_article_hint}\n\n{}\n\n## Current context\n{}\n\n## User request\n{}\n",
        tools_catalog(),
        context_blob,
        user.trim()
    )
}

/// Round-2 / retry: force a Reader-visible final answer — no tool catalog, no tool JSON.
pub fn build_final_answer_prompt(user: &str, tool_blob: &str, article_excerpt: &str, lang_rule: &str) -> String {
    let article_block = if article_excerpt.trim().is_empty() {
        String::new()
    } else {
        format!("\n\nOpen article (title + body excerpt):\n{article_excerpt}\n")
    };
    format!(
        r#"No tools this turn. Do not emit tool JSON / ```json / {{"tools":...}} / {{"name":...}}.
Answer the user's request directly in whatever format they asked for.

User request:
{user}
{article_block}
Tool results (if any — use them; do not dump raw JSON):
{tool_blob}

Rules:
- Follow the user request; do not force a fixed shape or graph style.
- {lang_rule}
- No tool calls. Do not leave the answer empty."#
    )
}

/// Direct CLI prompt with article body only (no tool catalog) — last-resort for open-article asks.
pub fn build_article_only_prompt(user: &str, title: &str, body: &str, lang_rule: &str) -> String {
    format!(
        r#"You are the Feedcairn Reader assistant. Answer from the open article below.
No tool JSON in the reply.

Open article:
Title: {title}

Body:
{body}

User request:
{user}

Follow the user's request freely (any format they ask for). {lang_rule} Do not emit ```json / {{"tools" / {{"name". Do not leave the answer empty."#
    )
}

pub fn parse_tool_calls(output: &str) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    // Prefer fenced ```json blocks, then whole-text JSON, then TOOL lines.
    for block in extract_json_blobs(output) {
        push_from_value(&mut out, &block);
    }
    if out.is_empty() {
        if let Ok(v) = serde_json::from_str::<Value>(output.trim()) {
            push_from_value(&mut out, &v);
        }
    }
    out
}

fn extract_json_blobs(text: &str) -> Vec<Value> {
    let mut values = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("```") {
        let after = &rest[start + 3..];
        let after = after
            .strip_prefix("json")
            .or_else(|| after.strip_prefix("JSON"))
            .unwrap_or(after);
        let after = after.strip_prefix('\n').unwrap_or(after);
        if let Some(end) = after.find("```") {
            let body = after[..end].trim();
            if let Ok(v) = serde_json::from_str::<Value>(body) {
                values.push(v);
            }
            rest = &after[end + 3..];
        } else {
            break;
        }
    }
    // Also catch a bare {"tools":...} substring.
    if values.is_empty() {
        if let Some(idx) = text.find("{\"tools\"") {
            if let Some(v) = try_parse_balanced(&text[idx..]) {
                values.push(v);
            }
        } else if let Some(idx) = text.find("{\"name\"") {
            if let Some(v) = try_parse_balanced(&text[idx..]) {
                values.push(v);
            }
        }
    }
    values
}

fn try_parse_balanced(s: &str) -> Option<Value> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&s[..=i]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

fn push_from_value(out: &mut Vec<(String, Value)>, v: &Value) {
    if let Some(arr) = v.get("tools").and_then(|x| x.as_array()) {
        for item in arr {
            if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                let args = item.get("args").cloned().unwrap_or(Value::Object(Default::default()));
                out.push((name.to_string(), args));
            }
        }
        return;
    }
    if let Some(name) = v.get("name").and_then(|n| n.as_str()) {
        let args = v
            .get("args")
            .cloned()
            .unwrap_or_else(|| v.get("arguments").cloned().unwrap_or(Value::Object(Default::default())));
        out.push((name.to_string(), args));
    }
}

pub async fn execute_tool(
    state: &AppState,
    name: &str,
    args: &Value,
    ctx: &AskAiContext,
) -> ToolCallResult {
    if !ALLOWED.contains(&name) {
        return ToolCallResult {
            name: name.to_string(),
            ok: false,
            detail: format!("unknown tool `{name}` (fail closed)"),
        };
    }
    match run_allowed(state, name, args, ctx).await {
        Ok(detail) => ToolCallResult {
            name: name.to_string(),
            ok: true,
            detail,
        },
        Err(e) => ToolCallResult {
            name: name.to_string(),
            ok: false,
            detail: e.to_string(),
        },
    }
}

async fn run_allowed(
    state: &AppState,
    name: &str,
    args: &Value,
    ctx: &AskAiContext,
) -> Result<String> {
    match name {
        "summarize_article" => {
            let id = arg_i64(args, "article_id")
                .or(ctx.article_id)
                .ok_or_else(|| Error::msg("summarize_article needs article_id"))?;
            // SQLite only — outer ask_ai round-2 (or synthesize) produces the summary.
            article_payload_for_summary(state, id).await
        }
        "analyze_feeds" => {
            let folder_id = arg_i64(args, "folder_id").or(ctx.folder_id);
            let limit = arg_i64(args, "limit").unwrap_or(40).clamp(5, 80);
            let (label, items) = state.with_db(|conn| {
                let label = if let Some(fid) = folder_id {
                    db::list_folders(conn)?
                        .into_iter()
                        .find(|f| f.id == fid)
                        .map(|f| f.name)
                        .unwrap_or_else(|| format!("folder {fid}"))
                } else {
                    "all feeds".into()
                };
                let items = db::list_articles(
                    conn,
                    &ArticleFilter {
                        folder_id,
                        unread_only: true,
                        limit: Some(limit),
                        ..Default::default()
                    },
                )?;
                Ok((label, items))
            })?;
            articles_structured_text(state, &label, &items)
        }
        "analyze_selection" => {
            let mut filter = ArticleFilter {
                unread_only: true,
                limit: Some(40),
                ..Default::default()
            };
            if let Some(id) = ctx.feed_id {
                filter.feed_id = Some(id);
                filter.unread_only = false;
            }
            if let Some(id) = ctx.folder_id {
                filter.folder_id = Some(id);
            }
            if ctx.selection_kind.as_deref() == Some("saved") {
                filter.saved_only = true;
                filter.unread_only = false;
            }
            if let Some(q) = ctx.query.as_ref().filter(|s| !s.trim().is_empty()) {
                filter.query = Some(q.clone());
            }
            let label = ctx
                .selection_label
                .clone()
                .unwrap_or_else(|| "current selection".into());
            let items = state.with_db(|conn| db::list_articles(conn, &filter))?;
            articles_structured_text(state, &label, &items)
        }
        "analyze_search" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| ctx.query.clone())
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| Error::msg("analyze_search needs query"))?;
            let items = state.with_db(|conn| {
                db::list_articles(
                    conn,
                    &ArticleFilter {
                        query: Some(query.clone()),
                        limit: Some(40),
                        ..Default::default()
                    },
                )
            })?;
            articles_structured_text(state, &format!("search:{query}"), &items)
        }
        "suggest_feeds" => {
            let topic = args
                .get("topic")
                .and_then(|v| v.as_str())
                .unwrap_or("the user's current reading interests");
            // SQLite only — round-2 writes suggestions from this catalog.
            let feeds_blob = state.with_db(|conn| {
                let feeds = db::list_feeds(conn)?;
                let mut s = String::new();
                for f in feeds.iter().take(40) {
                    s.push_str(&format!("- {} ({})\n", f.title, f.url));
                }
                Ok(s)
            })?;
            Ok(format!(
                "suggest_feeds topic: {topic}\n\nAlready subscribed:\n{feeds_blob}\n\n(Host will draft 5–10 RSS suggestions in the user's language from this list — no nested AI here.)"
            ))
        }
        "search_rss_web" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| Error::msg("search_rss_web needs query"))?
                .to_string();
            let months = arg_i64(args, "months").unwrap_or(3).clamp(1, 12);
            search_rss_web_impl(&query, months).await
        }
        "add_feed" => {
            let url = args
                .get("url")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| Error::msg("add_feed needs url"))?
                .to_string();
            let folder_id = arg_i64(args, "folder_id").or(ctx.folder_id);
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let feed = state
                .add_feed_and_fetch(AddFeedRequest {
                    url,
                    folder_id,
                    title,
                })
                .await?;
            Ok(format!("subscribed to {} (id {})", feed.title, feed.id))
        }
        "write_note" => {
            let body = args
                .get("body_md")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if body.trim().is_empty() {
                return Err(Error::msg("write_note needs body_md"));
            }
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("AI note")
                .to_string();
            let note_id = arg_i64(args, "note_id");
            let append = args
                .get("append")
                .and_then(|v| v.as_bool())
                .unwrap_or(note_id.is_some());
            let note = state.with_db(|conn| {
                if let Some(id) = note_id {
                    if append {
                        db::append_note(conn, id, &body)
                    } else {
                        db::update_note(conn, id, Some(&title), Some(&body))
                    }
                } else {
                    db::create_note(conn, &title, &body)
                }
            })?;
            Ok(format!("note #{} «{}» ({} chars)", note.id, note.title, note.body_md.len()))
        }
        "list_notes" => {
            let notes = state.with_db(db::list_notes)?;
            if notes.is_empty() {
                return Ok("No notes yet.".into());
            }
            let mut s = String::new();
            for n in notes {
                s.push_str(&format!("- #{} {} (updated {})\n", n.id, n.title, n.updated_at));
            }
            Ok(s)
        }
        _ => Err(Error::msg("unreachable")),
    }
}



/// Prefer longer of content_html plain text vs summary (teaser bodies).
fn best_article_body(content_html: &str, summary: &str) -> String {
    let from_html = html2text::from_read(content_html.as_bytes(), 80).unwrap_or_default();
    let from_summary = summary.trim().to_string();
    let html_len = from_html.chars().count();
    let sum_len = from_summary.chars().count();
    if html_len == 0 && sum_len == 0 {
        String::new()
    } else if html_len >= sum_len {
        from_html
    } else {
        from_summary
    }
}

/// Public body excerpt for ask_ai retries / local fallback (plain text).
pub fn public_article_body_excerpt(article: &Article, max_chars: usize) -> String {
    truncate_chars(
        &best_article_body(&article.content_html, &article.summary),
        max_chars,
    )
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

fn articles_structured_text(
    state: &AppState,
    label: &str,
    items: &[ArticleListItem],
) -> Result<String> {
    if items.is_empty() {
        return Ok(format!("[{label}] No matching articles."));
    }
    let mut out = format!("[{label}] {} makale:\n", items.len());
    for item in items.iter().take(40) {
        let excerpt = state
            .with_db(|conn| {
                if let Ok(a) = db::get_article(conn, item.id) {
                    Ok(truncate_chars(&best_article_body(&a.content_html, &a.summary), 700))
                } else {
                    Ok(truncate_chars(&item.summary, 700))
                }
            })
            .unwrap_or_else(|_| truncate_chars(&item.summary, 700));
        out.push_str(&format!(
            "- id={} | {} | feed={} | url={}\n  {}\n",
            item.id,
            item.title,
            item.feed_title,
            item.url.as_deref().unwrap_or(""),
            excerpt.replace('\n', " ")
        ));
    }
    Ok(out)
}

async fn article_payload_for_summary(state: &AppState, id: i64) -> Result<String> {
    // Optionally expand short teasers via load_full before returning body text.
    let article = state.with_db(|c| db::get_article(c, id))?;
    let plain_len = {
        let body = best_article_body(&article.content_html, &article.summary);
        body.chars().count()
    };
    let article = if plain_len < 1500
        && article
            .url
            .as_ref()
            .map(|u| !u.trim().is_empty())
            .unwrap_or(false)
    {
        match state.load_full_article(id).await {
            Ok(full) => full,
            Err(_) => article,
        }
    } else {
        article
    };
    let body = truncate_chars(
        &best_article_body(&article.content_html, &article.summary),
        8000,
    );
    Ok(format!(
        "article_id: {}\ntitle: {}\nurl: {}\nfeed: {}\n\n{}",
        article.id,
        article.title,
        article.url.as_deref().unwrap_or(""),
        article.feed_title,
        body
    ))
}

/// True when stripped visible prose is empty (only tool JSON / fences).
pub fn visible_prose_empty(output: &str) -> bool {
    strip_tool_artifacts_rs(output).trim().is_empty()
}

/// Lightweight Rust mirror of frontend stripToolArtifacts — enough to detect empty Reader text.
pub fn strip_tool_artifacts_rs(raw: &str) -> String {
    let mut s = raw.replace("\r\n", "\n");
    // Drop only tool-ish fences. Keep mermaid / svg / html for graph answers.
    let mut out = String::new();
    let mut rest = s.as_str();
    while let Some(start) = rest.find("```") {
        out.push_str(&rest[..start]);
        let after_ticks = &rest[start + 3..];
        if let Some(end_rel) = after_ticks.find("```") {
            let fence_inner = &after_ticks[..end_rel];
            let (lang, body) = if let Some(nl) = fence_inner.find('\n') {
                (
                    fence_inner[..nl].trim().to_lowercase(),
                    fence_inner[nl + 1..].trim(),
                )
            } else {
                (String::new(), fence_inner.trim())
            };
            let is_graph = matches!(lang.as_str(), "mermaid" | "svg" | "html");
            let is_tool_fence = lang == "json"
                || lang == "javascript"
                || lang == "js"
                || body.contains("\"tools\"")
                || ALLOWED.iter().any(|n| {
                    body.contains(&format!("\"name\":\"{n}\""))
                        || body.contains(&format!("\"name\": \"{n}\""))
                });
            let fence_end = start + 3 + end_rel + 3;
            if is_graph || !is_tool_fence {
                out.push_str(&rest[start..fence_end]);
            } else {
                out.push('\n');
            }
            rest = &rest[fence_end..];
        } else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    s = out;

    // Bare {"tools":...} or {"name":"<tool>"...} (single tool object).
    loop {
        let idx_tools = s.find("{\"tools\"");
        let idx_name = s.find("{\"name\"");
        let idx = match (idx_tools, idx_name) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        let Some(idx) = idx else { break };
        let Some(end_idx) = balanced_end(&s, idx) else { break };
        let slice = &s[idx..=end_idx];
        let is_toolish = slice.contains("\"tools\"")
            || ALLOWED.iter().any(|n| slice.contains(&format!("\"name\":\"{n}\"")))
            || ALLOWED.iter().any(|n| slice.contains(&format!("\"name\": \"{n}\"")));
        if !is_toolish {
            break;
        }
        s = format!("{}\n{}", &s[..idx], &s[end_idx + 1..]);
    }
    s.trim().to_string()
}

fn balanced_end(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Reuse tool text as Reader output only when it is already finished prose.
/// Raw SQLite dumps (article_id:/id= title) must go through round-2 instead.
pub fn synthesize_output_from_tools(results: &[ToolCallResult]) -> Option<String> {
    let mut parts = Vec::new();
    for t in results {
        if !t.ok {
            continue;
        }
        let detail = t.detail.trim();
        if detail.is_empty() || detail.chars().count() < 80 {
            continue;
        }
        if detail.trim_start().starts_with('{') || detail.contains("\"tools\"") {
            continue;
        }
        // Structured DB payloads — need round-2 to become Reader prose.
        if detail.contains("article_id:")
            || detail.contains("makale:")
            || detail.contains("Already subscribed:")
            || detail.contains("search_rss_web query=")
            || detail.contains("---ITEM---")
            || detail.lines().any(|l| l.trim_start().starts_with("- id="))
        {
            continue;
        }
        let looks_finished = detail.lines().any(|l| {
            let t = l.trim_start();
            t.starts_with("- ")
                || t.starts_with("* ")
                || t.starts_with('#')
                || t.starts_with("özet")
        }) || detail.chars().count() > 200;
        if !looks_finished {
            continue;
        }
        if matches!(
            t.name.as_str(),
            "summarize_article"
                | "analyze_search"
                | "analyze_feeds"
                | "analyze_selection"
                | "suggest_feeds"
                | "list_notes"
        ) {
            parts.push(detail.to_string());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

pub fn format_tool_results_for_round2(results: &[ToolCallResult]) -> String {
    let mut s = String::new();
    for t in results {
        s.push_str(&format!(
            "### tool {} ({})\n{}\n\n",
            t.name,
            if t.ok { "ok" } else { "err" },
            t.detail
        ));
    }
    s
}



async fn search_rss_web_impl(query: &str, months: i64) -> Result<String> {
    let days = (months * 30).clamp(30, 365) as i64;
    let us_url = google_news_rss_url(query, days, "en", "US", "US:en");
    let tr_url = google_news_rss_url(query, days, "tr", "TR", "TR:tr");

    let mut hits: Vec<RssWebHit> = Vec::new();
    let mut errs: Vec<String> = Vec::new();

    for (label, feed_url) in [("US", us_url.as_str()), ("TR", tr_url.as_str())] {
        match fetch_google_news_hits(feed_url, months).await {
            Ok(mut batch) => {
                for h in &mut batch {
                    if h.feed_url.is_empty() {
                        h.feed_url = feed_url.to_string();
                    }
                }
                hits.append(&mut batch);
            }
            Err(e) => errs.push(format!("{label}: {e}")),
        }
    }

    // Prefer US search URL as the durable subscribe target for the query.
    let subscribe_url = us_url.clone();
    for h in &mut hits {
        h.feed_url = subscribe_url.clone();
    }

    // Dedupe by link, then title.
    let mut seen_link = std::collections::HashSet::new();
    let mut seen_title = std::collections::HashSet::new();
    hits.retain(|h| {
        let link_key = h.link.trim().to_lowercase();
        let title_key = h.title.trim().to_lowercase();
        if !link_key.is_empty() {
            if !seen_link.insert(link_key) {
                return false;
            }
        }
        if !title_key.is_empty() && !seen_title.insert(title_key) {
            return false;
        }
        true
    });
    hits.truncate(18);

    let mut out = format!(
        "search_rss_web query={query:?} months={months}\nSEARCH_FEED_URL: {subscribe_url}\nTR_FEED_URL: {tr_url}\nitems: {}\n",
        hits.len()
    );
    if hits.is_empty() {
        out.push_str("\n(No recent Google News RSS items for this query in the window.)\n");
        if !errs.is_empty() {
            out.push_str(&format!("fetch_notes: {}\n", errs.join("; ")));
        }
        return Ok(out);
    }
    for h in &hits {
        out.push_str(&format!(
            "\n---ITEM---\nTITLE: {}\nLINK: {}\nPUBLISHED: {}\nSOURCE: {}\nSUMMARY: {}\nFEED_URL: {}\n---END---\n",
            one_line(&h.title),
            one_line(&h.link),
            one_line(&h.published),
            one_line(&h.source),
            one_line(&h.summary),
            one_line(&h.feed_url),
        ));
    }
    if !errs.is_empty() {
        out.push_str(&format!("\nfetch_notes: {}\n", errs.join("; ")));
    }
    Ok(out)
}

struct RssWebHit {
    title: String,
    link: String,
    published: String,
    source: String,
    summary: String,
    feed_url: String,
}

fn google_news_rss_url(query: &str, days: i64, hl: &str, gl: &str, ceid: &str) -> String {
    let q = format!("{query} when:{days}d");
    let mut url = url::Url::parse("https://news.google.com/rss/search").expect("google news base");
    url.query_pairs_mut()
        .append_pair("q", &q)
        .append_pair("hl", hl)
        .append_pair("gl", gl)
        .append_pair("ceid", ceid);
    url.to_string()
}

async fn fetch_google_news_hits(feed_url: &str, months: i64) -> Result<Vec<RssWebHit>> {
    let outcome = crate::fetch::fetch_feed(feed_url, None, None).await?;
    let body = match outcome {
        crate::fetch::FetchOutcome::NotModified => {
            return Err(Error::msg("unexpected NotModified without validators"));
        }
        crate::fetch::FetchOutcome::Body(b) => b.body,
    };
    let parsed = feed_rs::parser::parse(body.as_slice())
        .map_err(|e| Error::msg(format!("RSS parse failed: {e}")))?;
    let cutoff = chrono::Utc::now() - chrono::Duration::days((months * 30).clamp(30, 365));
    let feed_title = parsed
        .title
        .as_ref()
        .map(|t| t.content.trim().to_string())
        .unwrap_or_default();

    let mut hits = Vec::new();
    for entry in &parsed.entries {
        let published_dt = entry.published.or(entry.updated);
        if let Some(dt) = published_dt {
            if dt < cutoff {
                continue;
            }
        }
        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "(untitled)".into());
        let link = entry
            .links
            .iter()
            .find(|l| l.rel.as_deref() == Some("alternate"))
            .or(entry.links.first())
            .map(|l| l.href.clone())
            .unwrap_or_default();
        let mut source = entry
            .source
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_default();
        if source.is_empty() {
            if let Some((_, right)) = title.rsplit_once(" - ") {
                let cand = right.trim();
                if !cand.is_empty() && cand.chars().count() < 80 {
                    source = cand.to_string();
                }
            }
        }
        if source.is_empty() && !feed_title.is_empty() {
            source = feed_title.clone();
        }
        let raw_summary = entry
            .summary
            .as_ref()
            .map(|t| t.content.as_str())
            .unwrap_or("");
        let raw_content = entry
            .content
            .as_ref()
            .and_then(|c| c.body.as_deref())
            .unwrap_or("");
        let summary = two_sentence_summary(if raw_summary.trim().is_empty() {
            raw_content
        } else {
            raw_summary
        });
        let published = published_dt
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        hits.push(RssWebHit {
            title,
            link,
            published,
            source,
            summary,
            feed_url: feed_url.to_string(),
        });
        if hits.len() >= 20 {
            break;
        }
    }
    Ok(hits)
}

fn two_sentence_summary(raw: &str) -> String {
    let plain = if raw.trim_start().starts_with('<') {
        html2text::from_read(raw.as_bytes(), 80).unwrap_or_default()
    } else {
        raw.to_string()
    };
    let collapsed = plain
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if collapsed.is_empty() {
        return String::new();
    }
    let mut sentences: Vec<String> = Vec::new();
    let mut buf = String::new();
    for ch in collapsed.chars() {
        buf.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            let s = buf.trim().to_string();
            if !s.is_empty() {
                sentences.push(s);
            }
            buf.clear();
            if sentences.len() >= 2 {
                break;
            }
        }
    }
    if sentences.len() < 2 {
        let rest = buf.trim();
        if !rest.is_empty() {
            sentences.push(rest.to_string());
        }
    }
    let joined = sentences.into_iter().take(2).collect::<Vec<_>>().join(" ");
    truncate_chars(&joined, 320)
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}


fn arg_i64(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_u64().map(|u| u as i64))
            .or_else(|| v.as_f64().map(|f| f as i64))
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}

pub fn context_blob(state: &AppState, ctx: &AskAiContext) -> Result<String> {
    state.with_db(|conn| {
        let mut parts = Vec::new();
        if let Some(kind) = &ctx.selection_kind {
            parts.push(format!(
                "selection: {kind} {}",
                ctx.selection_label.as_deref().unwrap_or("")
            ));
        }
        if let Some(id) = ctx.folder_id {
            parts.push(format!("folder_id: {id}"));
        }
        if let Some(id) = ctx.feed_id {
            parts.push(format!("feed_id: {id}"));
        }
        if let Some(q) = &ctx.query {
            if !q.trim().is_empty() {
                parts.push(format!("search_query: {q}"));
            }
        }
        if let Some(id) = ctx.article_id {
            if let Ok(a) = db::get_article(conn, id) {
                // Prefer a large body slice so open-article asks (graph/nodes) need no tool round.
                let excerpt = truncate_chars(
                    &best_article_body(&a.content_html, &a.summary),
                    6000,
                );
                parts.push(format!(
                    "article_id: {}\ntitle: {}\nurl: {}\nfeed: {}\nbody:\n{}",
                    a.id,
                    a.title,
                    a.url.as_deref().unwrap_or(""),
                    a.feed_title,
                    excerpt
                ));
            }
        }
        let feeds = db::list_feeds(conn)?;
        let mut feed_lines = String::from("subscribed_feeds (sample):\n");
        for f in feeds.iter().take(25) {
            feed_lines.push_str(&format!("- id={} {} unread={} url={}\n", f.id, f.title, f.unread, f.url));
        }
        parts.push(feed_lines);
        let notes = db::list_notes(conn)?;
        if !notes.is_empty() {
            let mut nl = String::from("notes:\n");
            for n in notes.iter().take(15) {
                nl.push_str(&format!("- id={} {}\n", n.id, n.title));
            }
            parts.push(nl);
        }
        Ok(parts.join("\n\n"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_tool_not_in_allowlist() {
        assert!(!ALLOWED.contains(&"rm_rf"));
        assert!(ALLOWED.contains(&"write_note"));
    }

    #[test]
    fn parses_fenced_tools_block() {
        let text = r#"Sure.

```json
{"tools":[{"name":"list_notes","args":{}},{"name":"write_note","args":{"title":"T","body_md":"hi"}}]}
```
"#;
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, "list_notes");
        assert_eq!(calls[1].0, "write_note");
    }

    #[test]
    fn parses_single_tool_object() {
        let text = r#"{"name":"add_feed","args":{"url":"https://example.com/rss"}}"#;
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "add_feed");
    }

    #[test]
    fn tool_only_json_is_visibly_empty() {
        let raw = r#"```json
{"tools":[{"name":"analyze_search","args":{"query":"zeynep"}}]}
```"#;
        assert!(visible_prose_empty(raw));
        assert!(visible_prose_empty(
            r#"{"name":"summarize_article","args":{"article_id":12}}"#
        ));
        assert!(!visible_prose_empty(
            "```mermaid\ngraph TD\n  A-->B\n```"
        ));
        assert!(!visible_prose_empty("İşte kısa bir özet:
- bir
- iki"));
    }

    #[test]
    fn synthesize_skips_raw_article_payload() {
        let results = vec![ToolCallResult {
            name: "summarize_article".into(),
            ok: true,
            detail: "article_id: 1
title: Hello
url: https://x
feed: F

".to_string()
                + &"body word ".repeat(40),
        }];
        assert!(synthesize_output_from_tools(&results).is_none());
    }

    #[test]
    fn synthesize_uses_finished_summary_prose() {
        let results = vec![ToolCallResult {
            name: "summarize_article".into(),
            ok: true,
            detail: "özet (stub)
- birinci madde

- ikinci madde

- üçüncü madde uzun açıklama burada".into(),
        }];
        let s = synthesize_output_from_tools(&results).expect("synth");
        assert!(s.contains("birinci madde"));
    }

    #[test]
    fn best_body_prefers_longer() {
        let short_html = "<p>kısa</p>";
        let long_sum = "bu özet çok daha uzun bir teaser metnidir ve tercih edilmelidir";
        let body = best_article_body(short_html, long_sum);
        assert!(body.contains("tercih"));
    }
}
