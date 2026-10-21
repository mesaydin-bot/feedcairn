use serde::{Deserialize, Serialize};

/// What the UI may show for a CLI: its file name only (`kimi`, `claude`), never the full
/// path, which usually contains the user's home folder.
pub fn display_command(command: &str) -> String {
    command.rsplit(['/', '\\']).next().unwrap_or(command).to_string()
}

/// Replace the user's home folder with `~` in text that reaches the UI (hints, CLI errors).
pub fn mask_home(text: &str) -> String {
    match dirs::home_dir().map(|h| h.to_string_lossy().trim_end_matches(['/', '\\']).to_string()) {
        Some(home) if home.len() > 1 => text.replace(&home, "~"),
        _ => text.to_string(),
    }
}

fn ser_command<S: serde::Serializer>(command: &str, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&display_command(command))
}

fn ser_masked<S: serde::Serializer>(text: &str, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&mask_home(text))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub sort: i64,
    pub unread: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feed {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub site_url: Option<String>,
    pub folder_id: Option<i64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub last_fetched: Option<i64>,
    pub last_error: Option<String>,
    pub unread: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    pub id: i64,
    pub feed_id: i64,
    pub feed_title: String,
    pub folder_id: Option<i64>,
    pub guid: String,
    pub title: String,
    pub url: Option<String>,
    pub published: Option<i64>,
    pub content_html: String,
    pub summary: String,
    pub is_read: bool,
    pub is_saved: bool,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleListItem {
    pub id: i64,
    pub feed_id: i64,
    pub feed_title: String,
    pub title: String,
    pub url: Option<String>,
    pub published: Option<i64>,
    pub summary: String,
    pub is_read: bool,
    pub is_saved: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArticleFilter {
    pub feed_id: Option<i64>,
    pub folder_id: Option<i64>,
    #[serde(default)]
    pub saved_only: bool,
    #[serde(default)]
    pub unread_only: bool,
    pub query: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Counts {
    pub unread: i64,
    pub saved: i64,
    pub all: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderNode {
    pub folder: Folder,
    pub feeds: Vec<Feed>,
    pub children: Vec<FolderNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bootstrap {
    pub folders: Vec<FolderNode>,
    pub root_feeds: Vec<Feed>,
    pub counts: Counts,
    pub settings: Vec<Setting>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub folders_created: u32,
    pub feeds_added: u32,
    pub feeds_skipped: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshResult {
    pub feed_id: i64,
    pub inserted: u32,
    pub updated: u32,
    pub skipped: u32,
    pub not_modified: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshAllResult {
    pub results: Vec<RefreshResult>,
    pub feeds_ok: u32,
    pub feeds_failed: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiOutput {
    pub provider: String,
    #[serde(serialize_with = "ser_command")]
    pub command: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProbe {
    pub provider: String,
    #[serde(serialize_with = "ser_command")]
    pub command: String,
    pub found: bool,
    #[serde(serialize_with = "ser_masked")]
    pub hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddFeedRequest {
    pub url: String,
    pub folder_id: Option<i64>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateFeedRequest {
    pub title: Option<String>,
    pub folder_id: Option<i64>,
    #[serde(default)]
    pub unfile: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFolderRequest {
    pub name: String,
    pub parent_id: Option<i64>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: i64,
    pub title: String,
    pub body_md: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNoteRequest {
    pub title: String,
    #[serde(default)]
    pub body_md: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNoteRequest {
    pub title: Option<String>,
    pub body_md: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AskAiContext {
    pub article_id: Option<i64>,
    pub feed_id: Option<i64>,
    pub folder_id: Option<i64>,
    pub query: Option<String>,
    pub selection_kind: Option<String>,
    pub selection_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskAiRequest {
    pub prompt: String,
    #[serde(default)]
    pub context: AskAiContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskAiResponse {
    pub provider: String,
    #[serde(serialize_with = "ser_command")]
    pub command: String,
    pub output: String,
    pub tool_results: Vec<ToolCallResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    pub id: i64,
    pub name: String,
    pub body: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePromptTemplateRequest {
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePromptTemplateRequest {
    pub name: Option<String>,
    pub body: Option<String>,
}

