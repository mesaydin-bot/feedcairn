use crate::db;
use crate::error::{Error, Result};
use crate::fetch::{self, FetchOutcome};
use crate::ingest;
use crate::models::*;
use crate::opml;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub db_path: PathBuf,
}

impl AppState {
    pub fn open(path: PathBuf) -> Result<Self> {
        let conn = db::open(&path)?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
            db_path: path,
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|_| Error::msg("database lock poisoned"))
    }

    pub(crate) fn with_db<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = self.lock()?;
        f(&conn)
    }

    pub fn bootstrap(&self) -> Result<Bootstrap> {
        self.with_db(db::bootstrap)
    }

    pub fn create_folder(&self, req: CreateFolderRequest) -> Result<Folder> {
        self.with_db(|c| db::create_folder(c, &req.name, req.parent_id))
    }

    pub fn rename_folder(&self, id: i64, name: &str) -> Result<()> {
        self.with_db(|c| db::rename_folder(c, id, name))
    }

    pub fn delete_folder(&self, id: i64) -> Result<()> {
        self.with_db(|c| db::delete_folder(c, id))
    }

    pub fn add_feed_record(&self, req: AddFeedRequest) -> Result<(Feed, bool)> {
        let title = req
            .title
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| req.url.clone());
        self.with_db(|c| db::add_feed(c, req.url.trim(), &title, None, req.folder_id))
    }

    pub fn update_feed(&self, id: i64, req: UpdateFeedRequest) -> Result<Feed> {
        let folder = if req.unfile {
            Some(None)
        } else {
            req.folder_id.map(Some)
        };
        self.with_db(|c| db::update_feed(c, id, req.title.as_deref(), folder))
    }

    pub fn delete_feed(&self, id: i64) -> Result<()> {
        self.with_db(|c| db::delete_feed(c, id))
    }

    pub fn list_articles(&self, filter: ArticleFilter) -> Result<Vec<ArticleListItem>> {
        self.with_db(|c| db::list_articles(c, &filter))
    }

    pub fn get_article(&self, id: i64) -> Result<Article> {
        self.with_db(|c| db::get_article(c, id))
    }

    pub fn set_read(&self, id: i64, is_read: bool) -> Result<Article> {
        self.with_db(|c| db::set_read(c, id, is_read))
    }

    pub fn set_saved(&self, id: i64, is_saved: bool) -> Result<Article> {
        self.with_db(|c| db::set_saved(c, id, is_saved))
    }

    pub fn mark_all_read(&self, filter: ArticleFilter) -> Result<u64> {
        self.with_db(|c| db::mark_all_read(c, &filter))
    }

    pub fn import_opml(&self, xml: &str) -> Result<ImportResult> {
        self.with_db(|c| opml::import_opml(c, xml))
    }

    pub fn export_opml(&self) -> Result<String> {
        self.with_db(opml::export_opml)
    }

    pub fn list_settings(&self) -> Result<Vec<Setting>> {
        self.with_db(db::list_settings)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with_db(|c| db::set_setting(c, key, value))
    }

    pub fn probe_ai(&self) -> Result<AiProbe> {
        let map = self.with_db(crate::ai::settings_map)?;
        Ok(crate::ai::probe(&map))
    }

    pub async fn refresh_feed(&self, id: i64) -> Result<RefreshResult> {
        let feed = self.with_db(|c| db::get_feed(c, id))?;
        match fetch::fetch_feed(&feed.url, feed.etag.as_deref(), feed.last_modified.as_deref()).await
        {
            Ok(FetchOutcome::NotModified) => {
                self.with_db(|c| {
                    db::update_fetch_meta(c, id, None, None, db::now_unix(), None)
                })?;
                Ok(ingest::empty_refresh(id, true, None))
            }
            Ok(FetchOutcome::Body(body)) => {
                let parsed = feed_rs::parser::parse(body.body.as_slice());
                match parsed {
                    Ok(parsed) => {
                        let (inserted, updated, skipped) =
                            self.with_db(|c| ingest::ingest_parsed(c, id, &parsed))?;
                        self.with_db(|c| {
                            db::update_fetch_meta(
                                c,
                                id,
                                body.etag.as_deref(),
                                body.last_modified.as_deref(),
                                db::now_unix(),
                                None,
                            )
                        })?;
                        Ok(RefreshResult {
                            feed_id: id,
                            inserted,
                            updated,
                            skipped,
                            not_modified: false,
                            error: None,
                        })
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        self.with_db(|c| {
                            db::update_fetch_meta(c, id, None, None, db::now_unix(), Some(&msg))
                        })?;
                        Ok(RefreshResult {
                            feed_id: id,
                            inserted: 0,
                            updated: 0,
                            skipped: 0,
                            not_modified: false,
                            error: Some(msg),
                        })
                    }
                }
            }
            Err(e) => {
                let msg = e.to_string();
                self.with_db(|c| {
                    db::update_fetch_meta(c, id, None, None, db::now_unix(), Some(&msg))
                })?;
                Ok(RefreshResult {
                    feed_id: id,
                    inserted: 0,
                    updated: 0,
                    skipped: 0,
                    not_modified: false,
                    error: Some(msg),
                })
            }
        }
    }

    pub async fn refresh_many(&self, folder_id: Option<i64>) -> Result<RefreshAllResult> {
        crate::worker::refresh_many(self, folder_id).await
    }

    pub async fn add_feed_and_fetch(&self, req: AddFeedRequest) -> Result<Feed> {
        let url = req.url.trim().to_string();
        if url.is_empty() {
            return Err(Error::msg("Feed URL is required"));
        }
        let (feed, _created) = self.add_feed_record(AddFeedRequest {
            url: url.clone(),
            folder_id: req.folder_id,
            title: req.title,
        })?;
        let _ = self.refresh_feed(feed.id).await?;
        self.with_db(|c| db::get_feed(c, feed.id))
    }


    pub async fn load_full_article(&self, article_id: i64) -> Result<Article> {
        let article = self.with_db(|c| db::get_article(c, article_id))?;
        let url = article
            .url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::msg("Makalenin URL'si yok; tam metin yüklenemez. / The article has no URL; full text cannot be loaded."))?
            .to_string();

        let existing_plain = existing_body_plain_len(&article);
        let from_google_news = fetch::is_google_news_article_url(&url);
        // Resolve Google News → publisher URL (errors are bilingual TR / EN).
        let fetch_url = fetch::resolve_article_url(&url).await?;
        if fetch_url != url {
            // Persist publisher URL so later loads skip Google News decode / Original link works.
            let _ = self.with_db(|c| {
                c.execute(
                    "UPDATE articles SET url = ?1 WHERE id = ?2",
                    rusqlite::params![fetch_url, article_id],
                )?;
                Ok(())
            });
        }

        let html = fetch::fetch_html(&fetch_url).await?;
        let candidate_html = pick_best_extracted_html(&html, &fetch_url);

        if candidate_html.trim().is_empty() {
            if let Some(restored) = self.try_restore_shortened_article(&article).await? {
                return Ok(restored);
            }
            if from_google_news {
                return Err(Error::msg(
                    "Google News kaynağı çözülemedi veya yayıncı sayfası engelli/boş; okunabilir metin çıkarılamadı. / Could not resolve the Google News source, or the publisher page is blocked/empty; no readable text.",
                ));
            }
            if existing_plain > 0 {
                // Soft-keep: never clear existing body.
                return Ok(article);
            }
            return Err(Error::msg(
                "Yayıncı sayfasından okunabilir metin çıkarılamadı. Kaynak engelli veya boş olabilir. / No readable text could be extracted from the publisher page; it may be blocked or empty.",
            ));
        }

        let candidate_plain = html_plain_len(&candidate_html);
        if !is_meaningfully_longer(candidate_plain, existing_plain) {
            // Never shorten stored body (paywall/teaser extracts are often shorter).
            if let Some(restored) = self.try_restore_shortened_article(&article).await? {
                return Ok(restored);
            }
            if from_google_news {
                return Err(Error::msg(
                    "Google News kaynağı çözüldü ancak yayıncı sayfasından daha uzun metin alınamadı (sayfa engelli, paywall veya teaser olabilir). / Google News was resolved but the publisher page gave no longer text (blocked, paywall or teaser).",
                ));
            }
            return Ok(article);
        }

        let search_text =
            html2text::from_read(candidate_html.as_bytes(), 80).unwrap_or_default();

        self.with_db(|c| {
            db::update_article_full_html(c, article_id, &candidate_html, &search_text)?;
            db::get_article(c, article_id)
        })
    }

    /// Re-pull feed entry once, or promote summary, when page extract is not longer
    /// (helps articles already shortened by an older overwrite bug).
    async fn try_restore_shortened_article(&self, article: &Article) -> Result<Option<Article>> {
        let content_plain = html_plain_len(&article.content_html);

        let feed = self.with_db(|c| db::get_feed(c, article.feed_id))?;
        // Force full body (no conditional headers) so we can recover RSS content.
        if let Ok(FetchOutcome::Body(body)) = fetch::fetch_feed(&feed.url, None, None).await {
            if let Ok(parsed) = feed_rs::parser::parse(body.body.as_slice()) {
                if let Some(rss_html) =
                    ingest::find_entry_html(&parsed, &article.guid, article.url.as_deref())
                {
                    let rss_plain = html_plain_len(&rss_html);
                    if is_meaningfully_longer(rss_plain, content_plain) {
                        let search_text =
                            html2text::from_read(rss_html.as_bytes(), 80).unwrap_or_default();
                        let updated = self.with_db(|c| {
                            db::update_article_full_html(c, article.id, &rss_html, &search_text)?;
                            db::get_article(c, article.id)
                        })?;
                        return Ok(Some(updated));
                    }
                }
            }
        }

        let summary_html = ingest::html_from_summary_text(&article.summary);
        let summary_plain = html_plain_len(&summary_html);
        if is_meaningfully_longer(summary_plain, content_plain) {
            let search_text =
                html2text::from_read(summary_html.as_bytes(), 80).unwrap_or_default();
            let updated = self.with_db(|c| {
                db::update_article_full_html(c, article.id, &summary_html, &search_text)?;
                db::get_article(c, article.id)
            })?;
            return Ok(Some(updated));
        }

        Ok(None)
    }

    pub async fn summarize(&self, article_id: i64) -> Result<AiOutput> {
        let mut article = self.with_db(|conn| db::get_article(conn, article_id))?;
        // Prefer longer of content/summary; if both short and URL present, try full extract first.
        let plain = existing_body_plain_len(&article);
        if plain < 1500
            && article
                .url
                .as_ref()
                .map(|u| !u.trim().is_empty())
                .unwrap_or(false)
        {
            if let Ok(full) = self.load_full_article(article_id).await {
                article = full;
            }
        }
        let settings = self.with_db(crate::ai::settings_map)?;
        crate::ai::summarize_article_data(&settings, &article).await
    }

    pub async fn summarize_table(&self, article_id: i64) -> Result<AiOutput> {
        let mut article = self.with_db(|conn| db::get_article(conn, article_id))?;
        let plain = existing_body_plain_len(&article);
        if plain < 1500
            && article
                .url
                .as_ref()
                .map(|u| !u.trim().is_empty())
                .unwrap_or(false)
        {
            if let Ok(full) = self.load_full_article(article_id).await {
                article = full;
            }
        }
        let settings = self.with_db(crate::ai::settings_map)?;
        crate::ai::summarize_article_table_data(&settings, &article).await
    }

    pub async fn tags(&self, article_id: i64) -> Result<AiOutput> {
        let (settings, article) = self.with_db(|conn| {
            Ok((
                crate::ai::settings_map(conn)?,
                db::get_article(conn, article_id)?,
            ))
        })?;
        crate::ai::tag_article_data(&settings, &article).await
    }

    pub async fn brief(&self, folder_id: i64) -> Result<AiOutput> {
        let (settings, name, items) = self.with_db(|conn| {
            let settings = crate::ai::settings_map(conn)?;
            let folders = db::list_folders(conn)?;
            let name = folders
                .iter()
                .find(|f| f.id == folder_id)
                .map(|f| f.name.clone())
                .ok_or_else(|| Error::msg("Folder not found"))?;
            let items = db::unread_titles_in_folder(conn, folder_id, 40)?;
            Ok((settings, name, items))
        })?;
        crate::ai::folder_brief_data(&settings, &name, &items).await
    }

    pub fn list_notes(&self) -> Result<Vec<Note>> {
        self.with_db(db::list_notes)
    }

    pub fn get_note(&self, id: i64) -> Result<Note> {
        self.with_db(|c| db::get_note(c, id))
    }

    pub fn create_note(&self, req: CreateNoteRequest) -> Result<Note> {
        self.with_db(|c| db::create_note(c, &req.title, &req.body_md))
    }

    pub fn update_note(&self, id: i64, req: UpdateNoteRequest) -> Result<Note> {
        self.with_db(|c| db::update_note(c, id, req.title.as_deref(), req.body_md.as_deref()))
    }

    pub fn delete_note(&self, id: i64) -> Result<()> {
        self.with_db(|c| db::delete_note(c, id))
    }

    pub fn list_prompt_templates(&self) -> Result<Vec<PromptTemplate>> {
        self.with_db(db::list_prompt_templates)
    }

    pub fn create_prompt_template(&self, req: CreatePromptTemplateRequest) -> Result<PromptTemplate> {
        self.with_db(|c| db::create_prompt_template(c, &req.name, &req.body))
    }

    pub fn update_prompt_template(
        &self,
        id: i64,
        req: UpdatePromptTemplateRequest,
    ) -> Result<PromptTemplate> {
        self.with_db(|c| {
            db::update_prompt_template(c, id, req.name.as_deref(), req.body.as_deref())
        })
    }

    pub fn delete_prompt_template(&self, id: i64) -> Result<()> {
        self.with_db(|c| db::delete_prompt_template(c, id))
    }

    pub async fn ask_ai(&self, req: AskAiRequest) -> Result<AskAiResponse> {
        let prompt_text = req.prompt.trim();
        if prompt_text.is_empty() {
            return Err(Error::msg("Ask AI prompt is empty"));
        }
        let mut ctx = req.context;

        // General title/keyword resolve: if the prompt names an article, inject it into context.
        if ctx.article_id.is_none() {
            if let Some(id) = self.resolve_article_id_from_prompt(prompt_text)? {
                ctx.article_id = Some(id);
            }
        }

        // Slash / freeform ask: always use CLI + tools with the user's exact prompt.
        // Summarize / Table summary UI buttons still call summarize / summarize_table APIs (fixed templates).

        let context_blob = crate::tools::context_blob(self, &ctx)?;
        let settings = self.with_db(crate::ai::settings_map)?;

        // Open-article freeform: prefer one direct CLI call (user + body), tools optional —
        // avoid over-constraining Codex with the full tool catalog on simple Reader asks.
        if !is_web_rss_search_intent(prompt_text) {
            if let Some((title, body)) = article_title_body_for_ask(self, ctx.article_id) {
                if !body.trim().is_empty() {
                    let local = crate::tools::build_article_only_prompt(prompt_text, &title, &body, crate::prompts::language_rule(&settings));
                    let direct = crate::ai::run_cli(&settings, &local).await?;
                    let direct_calls = crate::tools::parse_tool_calls(&direct.output);
                    if direct_calls.is_empty()
                        && !crate::tools::visible_prose_empty(&direct.output)
                        && !looks_like_only_tool_json(&direct.output)
                    {
                        return Ok(AskAiResponse {
                            provider: direct.provider,
                            command: direct.command,
                            output: direct.output,
                            tool_results: vec![],
                        });
                    }
                    // If the model asked for tools, fall through to the normal tool path below.
                }
            }
        }

        let full_prompt = crate::tools::build_ask_prompt(prompt_text, &context_blob, crate::prompts::language_rule(&settings));

        // Round 1: CLI may emit tool JSON and/or prose.
        let raw = crate::ai::run_cli(&settings, &full_prompt).await?;
        let calls = crate::tools::parse_tool_calls(&raw.output);
        let mut tool_results = Vec::new();
        for (name, args) in calls {
            tool_results.push(crate::tools::execute_tool(self, &name, &args, &ctx).await);
        }

        // Host ensure: internet/RSS discovery must hit search_rss_web even if the model
        // called suggest_feeds / invented prose / skipped tools.
        if is_web_rss_search_intent(prompt_text)
            && !tool_results
                .iter()
                .any(|t| t.name == "search_rss_web" && t.ok)
        {
            let q = extract_web_search_query(prompt_text).unwrap_or_else(|| prompt_text.to_string());
            let months = extract_months_from_prompt(prompt_text);
            let args = serde_json::json!({ "query": q, "months": months });
            tool_results.push(
                crate::tools::execute_tool(self, "search_rss_web", &args, &ctx).await,
            );
        }

        let provider = raw.provider.clone();
        let mut command = raw.command.clone();
        let mut output = raw.output.clone();

        let tools_ran = !tool_results.is_empty();
        let prose_empty = crate::tools::visible_prose_empty(&output);
        let article_excerpt = article_excerpt_for_ask(self, ctx.article_id);

        if tools_ran || prose_empty {
            // Prefer synthesizing when a tool already returned finished Reader prose.
            if let Some(synth) = crate::tools::synthesize_output_from_tools(&tool_results) {
                output = synth;
            } else if tools_ran {
                let tool_blob = crate::tools::format_tool_results_for_round2(&tool_results);
                let has_web_search = tool_results.iter().any(|t| t.name == "search_rss_web");
                let article_extra = if article_excerpt.is_empty() {
                    String::new()
                } else {
                    format!("\nOpen article excerpt:\n{article_excerpt}\n")
                };
                let lang_rule = crate::prompts::language_rule(&settings);
                let round2 = if has_web_search {
                    format!(
                        r#"User request:
{prompt_text}

Tool results (search_rss_web / other):
{tool_blob}

Follow the user's request. {lang_rule} No tool JSON / raw tool dump; do not say you are "doing it" without data.
For search_rss_web ITEMs (when user did not specify another format): title link, short summary, date/source. If empty, say so. Do not invent FEED_URL lines — UI draws Subscribe from tool_results.
No tool JSON / ```json / {{"tools" / {{"name" in this reply — user-visible answer only."#
                    )
                } else {
                    format!(
                        r#"User request:
{prompt_text}

Tool results (SQLite / action output):
{tool_blob}
{article_extra}

Follow the user's request freely; do not force a fixed answer shape. {lang_rule} No tool JSON / raw article_id dump. Rewrite tool data into the answer they asked for — do not paste dumps.
No tool JSON / ```json / {{"tools" / {{"name" in this reply — user-visible answer only."#
                    )
                };
                match crate::ai::run_cli(&settings, &round2).await {
                    Ok(r2) => {
                        if crate::tools::visible_prose_empty(&r2.output)
                            || looks_like_only_tool_json(&r2.output)
                        {
                            output = fallback_output_from_tools(&tool_results);
                        } else {
                            output = r2.output;
                        }
                        if !r2.command.is_empty() {
                            command = r2.command;
                        }
                    }
                    Err(_) => {
                        output = fallback_output_from_tools(&tool_results);
                    }
                }

                // Stronger retry once when still empty / tool JSON / raw SQLite dump.
                if needs_final_answer_retry(&output) {
                    let tool_blob = crate::tools::format_tool_results_for_round2(&tool_results);
                    let strong = crate::tools::build_final_answer_prompt(
                        prompt_text,
                        &tool_blob,
                        &article_excerpt,
                        crate::prompts::language_rule(&settings),
                    );
                    if let Ok(r3) = crate::ai::run_cli(&settings, &strong).await {
                        if !crate::tools::visible_prose_empty(&r3.output)
                            && !looks_like_only_tool_json(&r3.output)
                        {
                            output = r3.output;
                            if !r3.command.is_empty() {
                                command = r3.command;
                            }
                        }
                    }
                }
            } else if prose_empty && !article_excerpt.is_empty() {
                // No tools parsed but open article in context — still try a final-answer pass.
                let strong = crate::tools::build_final_answer_prompt(
                    prompt_text,
                    "(no tool results — use the open article in context)",
                    &article_excerpt,
                    crate::prompts::language_rule(&settings),
                );
                if let Ok(r2) = crate::ai::run_cli(&settings, &strong).await {
                    if !crate::tools::visible_prose_empty(&r2.output)
                        && !looks_like_only_tool_json(&r2.output)
                    {
                        output = r2.output;
                        if !r2.command.is_empty() {
                            command = r2.command;
                        }
                    }
                }
            }
        }

        // Open-article local fallback: body + user format only (no tool catalog).
        if needs_final_answer_retry(&output) {
            if let Some((title, body)) = article_title_body_for_ask(self, ctx.article_id) {
                let local = crate::tools::build_article_only_prompt(prompt_text, &title, &body, crate::prompts::language_rule(&settings));
                if let Ok(r) = crate::ai::run_cli(&settings, &local).await {
                    if !crate::tools::visible_prose_empty(&r.output)
                        && !looks_like_only_tool_json(&r.output)
                    {
                        output = r.output;
                        if !r.command.is_empty() {
                            command = r.command;
                        }
                    }
                }
            }
        }

        // Never return a "success" that strips to empty in the Reader.
        if crate::tools::visible_prose_empty(&output) || looks_like_only_tool_json(&output) {
            if let Some(synth) = crate::tools::synthesize_output_from_tools(&tool_results) {
                output = synth;
            } else if !tool_results.is_empty() {
                let fb = fallback_output_from_tools(&tool_results);
                if !crate::tools::visible_prose_empty(&fb) && !looks_like_only_tool_json(&fb) {
                    output = fb;
                }
            }
            if crate::tools::visible_prose_empty(&output) || looks_like_only_tool_json(&output) {
                return Err(Error::msg(
                    "AI yanıtı boş kaldı (yalnızca araç JSON'u). Lütfen tekrar deneyin. / The AI answer was empty (tool JSON only). Please try again.",
                ));
            }
        }

        Ok(AskAiResponse {
            provider,
            command,
            output,
            tool_results,
        })
    }

    /// Search DB by prompt keywords; pick best title match. No hardcoded titles.
    fn resolve_article_id_from_prompt(&self, prompt: &str) -> Result<Option<i64>> {
        let keywords = extract_search_keywords(prompt);
        if keywords.is_empty() {
            return Ok(None);
        }
        // Collect hits per keyword (phrase JOIN often fails on punctuation/case).
        let mut by_id: std::collections::HashMap<i64, ArticleListItem> =
            std::collections::HashMap::new();
        let mut hit_counts: std::collections::HashMap<i64, i32> =
            std::collections::HashMap::new();

        let mut queries: Vec<String> = Vec::new();
        let joined = keywords.join(" ");
        if !joined.is_empty() {
            queries.push(joined);
        }
        for kw in &keywords {
            queries.push(kw.clone());
        }

        for q in queries {
            if q.trim().is_empty() {
                continue;
            }
            let items = self.with_db(|c| {
                db::list_articles(
                    c,
                    &ArticleFilter {
                        query: Some(q.clone()),
                        limit: Some(40),
                        ..Default::default()
                    },
                )
            })?;
            for item in items {
                *hit_counts.entry(item.id).or_insert(0) += 1;
                by_id.entry(item.id).or_insert(item);
            }
        }

        if by_id.is_empty() {
            return Ok(None);
        }

        let candidates: Vec<ArticleListItem> = by_id.into_values().collect();
        let best = pick_best_title_match(&candidates, &keywords);
        // Tie-break with multi-keyword hit count.
        let best_hits = hit_counts.get(&best.id).copied().unwrap_or(0);
        let mut chosen = best;
        for item in &candidates {
            let hits = hit_counts.get(&item.id).copied().unwrap_or(0);
            if hits > best_hits
                && title_keyword_score(item, &keywords) >= title_keyword_score(chosen, &keywords)
            {
                chosen = item;
            }
        }
        Ok(Some(chosen.id))
    }
}


fn looks_like_only_tool_json(output: &str) -> bool {
    let t = output.trim();
    if t.is_empty() {
        return true;
    }
    // After strip, empty ⇒ only tool artifacts.
    if crate::tools::visible_prose_empty(output) {
        return true;
    }
    // Whole response is a single tool object / tools array.
    if (t.starts_with('{') || t.starts_with("```"))
        && (t.contains("\"tools\"") || t.contains("\"name\""))
        && !crate::tools::parse_tool_calls(t).is_empty()
        && crate::tools::strip_tool_artifacts_rs(t).trim().chars().count() < 40
    {
        return true;
    }
    false
}

/// Raw SQLite / tool dumps are not finished Reader answers (need strong retry / local fallback).
fn is_raw_tool_payload(output: &str) -> bool {
    let t = output.trim_start();
    t.starts_with("article_id:")
        || t.contains("### tool ")
        || t.contains("search_rss_web query=")
        || t.contains("Already subscribed:")
        || t.contains("---ITEM---")
        || t.lines().any(|l| l.trim_start().starts_with("- id="))
}

fn needs_final_answer_retry(output: &str) -> bool {
    crate::tools::visible_prose_empty(output)
        || looks_like_only_tool_json(output)
        || is_raw_tool_payload(output)
}

fn article_excerpt_for_ask(state: &AppState, article_id: Option<i64>) -> String {
    let Some(id) = article_id else {
        return String::new();
    };
    state
        .with_db(|conn| {
            let a = db::get_article(conn, id)?;
            let body = crate::tools::public_article_body_excerpt(&a, 4000);
            Ok(format!("title: {}\n{}", a.title, body))
        })
        .unwrap_or_default()
}

fn article_title_body_for_ask(state: &AppState, article_id: Option<i64>) -> Option<(String, String)> {
    let id = article_id?;
    state
        .with_db(|conn| {
            let a = db::get_article(conn, id)?;
            let body = crate::tools::public_article_body_excerpt(&a, 8000);
            if body.trim().is_empty() {
                return Err(Error::msg("empty article body"));
            }
            Ok((a.title, body))
        })
        .ok()
}

fn fallback_output_from_tools(results: &[ToolCallResult]) -> String {
    if let Some(web) = results.iter().find(|t| t.ok && t.name == "search_rss_web") {
        return format_search_rss_fallback(&web.detail);
    }
    let joined = results
        .iter()
        .filter(|t| t.ok && !t.detail.trim().is_empty())
        .map(|t| t.detail.trim().to_string())
        .collect::<Vec<_>>()
        .join("

");
    if joined.trim().is_empty() {
        results
            .iter()
            .map(|t| format!("{}: {}", t.name, t.detail))
            .collect::<Vec<_>>()
            .join("
")
    } else {
        joined
    }
}

fn format_search_rss_fallback(detail: &str) -> String {
    let mut out = String::from("## İnternet RSS araması / Web RSS search

");
    let mut count = 0usize;
    for part in detail.split("---ITEM---").skip(1) {
        let body = part.split("---END---").next().unwrap_or(part);
        let mut title = String::new();
        let mut link = String::new();
        let mut published = String::new();
        let mut source = String::new();
        let mut summary = String::new();
        for line in body.lines() {
            if let Some(rest) = line.strip_prefix("TITLE: ") {
                title = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("LINK: ") {
                link = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("PUBLISHED: ") {
                published = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("SOURCE: ") {
                source = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("SUMMARY: ") {
                summary = rest.trim().to_string();
            }
        }
        if title.is_empty() {
            continue;
        }
        count += 1;
        if !link.is_empty() {
            out.push_str(&format!("{count}. [{title}]({link})
"));
        } else {
            out.push_str(&format!("{count}. **{title}**
"));
        }
        let meta = [source.as_str(), published.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        if !meta.is_empty() {
            out.push_str(&format!("   _{meta}_
"));
        }
        if !summary.is_empty() {
            out.push_str(&format!("   {summary}
"));
        }
        out.push('\n');
    }
    if count == 0 {
        return "İnternet RSS aramasında bu pencerede sonuç bulunamadı. / The web RSS search found nothing in this window.".into();
    }
    out.push_str("
Alttaki **Abone ol** ile arama feed'ini Feedcairn'e ekleyebilirsiniz.
");
    out
}

fn is_web_rss_search_intent(prompt: &str) -> bool {
    let p = prompt.to_lowercase();
    let webish = p.contains("internetten")
        || p.contains("internet ")
        || p.starts_with("internet")
        || p.contains("google news")
        || p.contains("web'de")
        || p.contains("webde")
        || p.contains("rss kaynak")
        || p.contains("rss feed")
        || p.contains("atom feed")
        || p.contains("search_rss")
        || (p.contains("rss")
            && (p.contains("ara")
                || p.contains("search")
                || p.contains("bul")
                || p.contains("kaynak")
                || p.contains("abone")));
    let monthsish = p.contains("son ") && (p.contains(" ay") || p.contains("month"));
    webish || (monthsish && (p.contains("rss") || p.contains("haber") || p.contains("news")))
}

fn extract_web_search_query(prompt: &str) -> Option<String> {
    if let Some(q) = extract_quoted_phrase(prompt) {
        let t = q.trim();
        if t.chars().count() >= 2 {
            return Some(t.to_string());
        }
    }
    const STOP: &[&str] = &[
        "internetten", "internet", "rss", "atom", "kaynaklarından", "kaynaklarindan",
        "kaynak", "kaynakları", "kaynaklari", "araması", "aramasi", "arama", "ara",
        "yap", "son", "ay", "aylık", "aylik", "için", "icin", "haberlerin", "haberler",
        "haber", "tamamını", "tamamini", "değil", "degil", "en", "fazla", "cümle",
        "cumle", "özetini", "ozetini", "özet", "ozet", "linkini", "link", "doğrudan",
        "dogrudan", "feedcairn", "abone", "olma", "seçeneğini", "secenegini", "seçenek",
        "secenek", "koy", "google", "news", "web", "search", "feed", "when", "months",
        "lütfen", "lutfen", "bana", "ve", "ile", "bir", "the", "a", "an", "from",
        "find", "look", "up", "please",
    ];
    let mut words: Vec<String> = Vec::new();
    for tok in prompt.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '\'')) {
        let t = tok.trim();
        if t.chars().count() < 2 {
            continue;
        }
        let lower = t.to_lowercase();
        if STOP.iter().any(|s| *s == lower.as_str()) {
            continue;
        }
        if lower.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        words.push(t.to_string());
    }
    if words.is_empty() {
        None
    } else {
        Some(words.join(" "))
    }
}

fn extract_quoted_phrase(prompt: &str) -> Option<String> {
    let pairs = [('"', '"'), ('\'', '\''), ('\u{201c}', '\u{201d}'), ('\u{00ab}', '\u{00bb}')];
    for (open, close) in pairs {
        if let Some(start) = prompt.find(open) {
            let rest = &prompt[start + open.len_utf8()..];
            if let Some(end) = rest.find(close) {
                let inner = rest[..end].trim();
                if !inner.is_empty() {
                    return Some(inner.to_string());
                }
            }
        }
    }
    None
}

fn extract_months_from_prompt(prompt: &str) -> i64 {
    let lower = prompt.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let n = chars.len();
    for i in 0..n {
        if chars[i].is_ascii_digit() {
            let mut j = i;
            while j < n && chars[j].is_ascii_digit() {
                j += 1;
            }
            let num: i64 = chars[i..j].iter().collect::<String>().parse().unwrap_or(0);
            if num <= 0 || num > 12 {
                continue;
            }
            let after: String = chars[j..].iter().skip_while(|c| c.is_whitespace()).take(12).collect();
            if after.starts_with("ay")
                || after.starts_with("month")
                || after.starts_with("aylık")
                || after.starts_with("aylik")
            {
                return num;
            }
        }
    }
    3
}

/// Strip intent/stop words; keep content tokens for DB search (any language / any title).
fn extract_search_keywords(prompt: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "özetle", "özet", "ozetle", "ozet", "summarize", "summary", "brief",
        "haberini", "haberi", "haber", "makaleyi", "makale", "article", "this",
        "bu", "şu", "su", "the", "a", "an", "ve", "ile", "için", "icin", "about",
        "bana", "lütfen", "lutfen", "please", "yaz", "anlat", "nedir", "ne",
        "kısaca", "kisaca", "oku", "open", "show", "göster", "goster",
        "tablo", "table", "şeklinde", "seklinde", "markdown", "gfm", "pipe",
        "olarak", "yanıtı", "yaniti", "yalnızca", "yalnizca", "gerçek", "gercek",
    ];
    prompt
        .split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '\''))
        .map(|s| s.trim())
        .filter(|s| s.chars().count() >= 2)
        .map(|s| s.to_lowercase())
        .filter(|s| !STOP.contains(&s.as_str()))
        .collect()
}

fn title_keyword_score(item: &ArticleListItem, keywords: &[String]) -> i32 {
    let title = item.title.to_lowercase();
    let summary = item.summary.to_lowercase();
    let mut score = 0i32;
    for kw in keywords {
        if title.contains(kw) {
            score += 10 + kw.chars().count() as i32;
        } else if summary.contains(kw) {
            score += 2;
        }
    }
    score
}

fn pick_best_title_match<'a>(items: &'a [ArticleListItem], keywords: &[String]) -> &'a ArticleListItem {
    let mut best = &items[0];
    let mut best_score = -1i32;
    for item in items {
        let score = title_keyword_score(item, keywords);
        if score > best_score {
            best_score = score;
            best = item;
        }
    }
    best
}



/// Plain-text length via html2text (char count).

fn html_plain_len(html: &str) -> usize {
    html2text::from_read(html.as_bytes(), 80)
        .unwrap_or_default()
        .chars()
        .count()
}

fn existing_body_plain_len(article: &Article) -> usize {
    let from_content = html_plain_len(&article.content_html);
    let from_summary = html_plain_len(&article.summary);
    from_content.max(from_summary)
}

/// Candidate must be at least 20% longer OR ≥ existing + 400 chars.
/// Short existing bodies (<800 chars): any strictly longer candidate wins
/// (Google News shells / teasers).
fn is_meaningfully_longer(candidate: usize, existing: usize) -> bool {
    if candidate == 0 {
        return false;
    }
    if existing == 0 {
        return true;
    }
    if existing < 800 && candidate > existing {
        return true;
    }
    candidate >= existing.saturating_add(existing / 5) || candidate >= existing.saturating_add(400)
}

/// Prefer the longer of readability extract and simple DOM fallbacks.
fn pick_best_extracted_html(page_html: &str, page_url: &str) -> String {
    let mut best = String::new();
    let mut best_len = 0usize;

    if let Ok(parsed_url) = url::Url::parse(page_url) {
        if let Ok(product) =
            readability::extractor::extract(&mut page_html.as_bytes(), &parsed_url)
        {
            let sanitized = ingest::sanitize_html(&product.content);
            let len = html_plain_len(&sanitized);
            if len > best_len {
                best_len = len;
                best = sanitized;
            }
        }
    }

    for chunk in fallback_content_chunks(page_html) {
        let sanitized = ingest::sanitize_html(&chunk);
        let len = html_plain_len(&sanitized);
        if len > best_len {
            best_len = len;
            best = sanitized;
        }
    }

    let _ = best_len;
    best
}

fn fallback_content_chunks(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tag in ["article", "main"] {
        if let Some(inner) = largest_tag_inner(html, tag) {
            out.push(inner);
        }
    }
    for needle in ["id=\"content\"", "id='content'", "id=\"Content\""] {
        if let Some(inner) = element_inner_near_attr(html, needle) {
            out.push(inner);
        }
    }
    for class in ["post-content", "entry-content", "article-content", "post-body"] {
        if let Some(inner) = element_inner_near_class(html, class) {
            out.push(inner);
        }
    }
    out
}

fn largest_tag_inner(html: &str, tag: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut best: Option<String> = None;
    let mut best_len = 0usize;
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find(&open) {
        let start = search_from + rel;
        let after = start + open.len();
        if after < lower.len() {
            let c = lower.as_bytes()[after];
            if !(c == b'>' || c == b' ' || c == b'\n' || c == b'\t' || c == b'\r' || c == b'/') {
                search_from = after;
                continue;
            }
        }
        let Some(gt_rel) = lower[after..].find('>') else {
            break;
        };
        let content_start = after + gt_rel + 1;
        let Some(end_rel) = lower[content_start..].find(&close) else {
            search_from = content_start;
            continue;
        };
        let content_end = content_start + end_rel;
        let chunk = html[content_start..content_end].to_string();
        let len = chunk.len();
        if len > best_len {
            best_len = len;
            best = Some(chunk);
        }
        search_from = content_end + close.len();
    }
    best
}

fn element_inner_near_attr(html: &str, attr_fragment: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let needle = attr_fragment.to_ascii_lowercase();
    let pos = lower.find(&needle)?;
    element_inner_from_attr_pos(html, &lower, pos)
}

fn element_inner_near_class(html: &str, class_name: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let class_name = class_name.to_ascii_lowercase();
    // Match class="… post-content …" / class='…'
    let mut search_from = 0;
    let mut best: Option<String> = None;
    let mut best_len = 0usize;
    while let Some(rel) = lower[search_from..].find("class=") {
        let abs = search_from + rel;
        let after_eq = abs + "class=".len();
        if after_eq >= lower.len() {
            break;
        }
        let quote = lower.as_bytes()[after_eq];
        if quote != b'"' && quote != b'\'' {
            search_from = after_eq;
            continue;
        }
        let val_start = after_eq + 1;
        let Some(val_end_rel) = lower[val_start..].find(quote as char) else {
            break;
        };
        let val_end = val_start + val_end_rel;
        let classes = &lower[val_start..val_end];
        let matched = classes.split_whitespace().any(|c| c == class_name);
        if matched {
            if let Some(inner) = element_inner_from_attr_pos(html, &lower, abs) {
                if inner.len() > best_len {
                    best_len = inner.len();
                    best = Some(inner);
                }
            }
        }
        search_from = val_end + 1;
    }
    best
}

fn element_inner_from_attr_pos(html: &str, lower: &str, attr_pos: usize) -> Option<String> {
    let open_lt = lower[..attr_pos].rfind('<')?;
    let tag_name_start = open_lt + 1;
    let tag_name_end = lower[tag_name_start..]
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .map(|i| tag_name_start + i)
        .unwrap_or(tag_name_start);
    if tag_name_end <= tag_name_start {
        return None;
    }
    let tag = &lower[tag_name_start..tag_name_end];
    if tag.is_empty() || tag.starts_with('!') || tag.starts_with('/') {
        return None;
    }
    let gt = lower[attr_pos..].find('>')? + attr_pos;
    // self-closing
    if gt > 0 && lower.as_bytes()[gt - 1] == b'/' {
        return None;
    }
    let content_start = gt + 1;
    let close = format!("</{tag}>");
    let end_rel = lower[content_start..].find(&close)?;
    let content_end = content_start + end_rel;
    Some(html[content_start..content_end].to_string())
}

#[cfg(test)]
mod full_article_tests {
    use super::*;

    #[test]
    fn meaningfully_longer_requires_20pct_or_400() {
        assert!(is_meaningfully_longer(100, 0));
        assert!(!is_meaningfully_longer(0, 100));
        // short existing (<800): any longer candidate wins
        assert!(is_meaningfully_longer(110, 100));
        assert!(is_meaningfully_longer(500, 400));
        assert!(!is_meaningfully_longer(400, 500));
        // long existing: still need +20% or +400
        assert!(!is_meaningfully_longer(1100, 1000)); // only +10%
        assert!(is_meaningfully_longer(1200, 1000)); // +20%
        assert!(is_meaningfully_longer(1500, 1000)); // +400
        assert!(!is_meaningfully_longer(3500, 3200)); // short of both thresholds
        assert!(is_meaningfully_longer(3840, 3200)); // +20%
        assert!(is_meaningfully_longer(3600, 3200)); // +400
    }

    #[test]
    fn shorter_candidate_does_not_overwrite_db() {
        let conn = db::open_memory().unwrap();
        let (feed, _) =
            db::add_feed(&conn, "https://example.com/rss.xml", "Ex", None, None).unwrap();
        let long_html = format!("<p>{}</p>", "kelime ".repeat(800)); // ~5600 chars plain-ish
        let search = html2text::from_read(long_html.as_bytes(), 80).unwrap_or_default();
        let id = db::insert_article(
            &conn,
            feed.id,
            "guid-1",
            "Title",
            Some("https://example.com/a"),
            None,
            &long_html,
            "summary",
            &search,
            0,
        )
        .unwrap();
        let before = db::get_article(&conn, id).unwrap();
        let before_len = before.content_html.len();

        // Simulate what load_full_article does when candidate is shorter:
        let short = "<p>paywall teaser</p>";
        let short_plain = html_plain_len(short);
        let existing_plain = existing_body_plain_len(&before);
        assert!(existing_plain > 3000);
        assert!(!is_meaningfully_longer(short_plain, existing_plain));
        // must NOT call update
        let after = db::get_article(&conn, id).unwrap();
        assert_eq!(after.content_html.len(), before_len);
        assert_eq!(after.content_html, before.content_html);
    }

    #[test]
    fn fallback_picks_article_tag() {
        let html = r#"<html><body><nav>nav</nav><article><p>Long article body with many words here.</p></article></body></html>"#;
        let chunks = fallback_content_chunks(html);
        assert!(chunks.iter().any(|c| c.contains("Long article body")));
    }

    #[test]
    fn summary_promote_when_content_was_shortened() {
        let conn = db::open_memory().unwrap();
        let (feed, _) =
            db::add_feed(&conn, "https://example.com/rss.xml", "Ex", None, None).unwrap();
        let short = "<p>teaser only</p>";
        let long_summary = format!("<p>{}</p>", "özet kelime ".repeat(200));
        let id = db::insert_article(
            &conn,
            feed.id,
            "guid-short",
            "Title",
            Some("https://example.com/b"),
            None,
            short,
            &long_summary,
            "teaser",
            0,
        )
        .unwrap();
        let article = db::get_article(&conn, id).unwrap();
        let content_plain = html_plain_len(&article.content_html);
        let summary_html = ingest::html_from_summary_text(&article.summary);
        let summary_plain = html_plain_len(&summary_html);
        assert!(is_meaningfully_longer(summary_plain, content_plain));
        let search = html2text::from_read(summary_html.as_bytes(), 80).unwrap_or_default();
        db::update_article_full_html(&conn, id, &summary_html, &search).unwrap();
        let after = db::get_article(&conn, id).unwrap();
        assert!(html_plain_len(&after.content_html) > content_plain);
    }
}

pub fn default_db_path() -> PathBuf {
    if let Ok(p) = std::env::var("FEEDCAIRN_DB") {
        return PathBuf::from(p);
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("feedcairn")
        .join("feedcairn.db")
}


#[cfg(test)]
mod ask_ai_helpers_tests {
    use super::*;

    #[test]
    fn extract_keywords_drops_stop_words() {
        let k = extract_search_keywords("zeynep çelik haberini özetle");
        assert!(k.iter().any(|w| w.contains("zeynep")));
        assert!(k.iter().any(|w| w.contains("çelik") || w.contains("celik")));
        assert!(!k.iter().any(|w| w == "özetle" || w == "haberini"));
    }

    #[test]
    fn web_rss_search_intent_and_extractors() {
        assert!(is_web_rss_search_intent(
            "internetten rss kaynaklarından \"track and trace systems\" araması yap son 3 ay için"
        ));
        assert!(is_web_rss_search_intent(
            "internetten rss kaynaklarından \"track and trace systems\" araması yap son 3 ay için, haberlerin tamamını değil en fazla 2 cümle özetini"
        ));
        assert!(!is_web_rss_search_intent("açık haberi node ve edges ler şeklinde relational tablo yap"));
        assert_eq!(
            extract_web_search_query(
                "internetten rss kaynaklarından \"track and trace systems\" araması yap son 3 ay için"
            )
            .as_deref(),
            Some("track and trace systems")
        );
        assert_eq!(extract_months_from_prompt("son 3 ay için ara"), 3);
    }
}
