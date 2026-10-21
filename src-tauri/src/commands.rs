use crate::app::AppState;
use crate::error::Error;
use crate::models::*;
use tauri::{Manager, State};

fn err(e: Error) -> String {
    serde_json::to_string(&e).unwrap_or_else(|_| e.to_string())
}

#[tauri::command]
fn bootstrap(state: State<AppState>) -> Result<Bootstrap, String> {
    state.bootstrap().map_err(err)
}

#[tauri::command]
fn create_folder(state: State<AppState>, req: CreateFolderRequest) -> Result<Folder, String> {
    state.create_folder(req).map_err(err)
}

#[tauri::command]
fn rename_folder(state: State<AppState>, id: i64, name: String) -> Result<(), String> {
    state.rename_folder(id, &name).map_err(err)
}

#[tauri::command]
fn delete_folder(state: State<AppState>, id: i64) -> Result<(), String> {
    state.delete_folder(id).map_err(err)
}

#[tauri::command]
async fn add_feed(state: State<'_, AppState>, req: AddFeedRequest) -> Result<Feed, String> {
    state.add_feed_and_fetch(req).await.map_err(err)
}

#[tauri::command]
fn update_feed(state: State<AppState>, id: i64, req: UpdateFeedRequest) -> Result<Feed, String> {
    state.update_feed(id, req).map_err(err)
}

#[tauri::command]
fn delete_feed(state: State<AppState>, id: i64) -> Result<(), String> {
    state.delete_feed(id).map_err(err)
}

#[tauri::command]
fn list_articles(state: State<AppState>, filter: ArticleFilter) -> Result<Vec<ArticleListItem>, String> {
    state.list_articles(filter).map_err(err)
}

#[tauri::command]
fn get_article(state: State<AppState>, id: i64) -> Result<Article, String> {
    state.get_article(id).map_err(err)
}

#[tauri::command]
fn set_read(state: State<AppState>, id: i64, is_read: bool) -> Result<Article, String> {
    state.set_read(id, is_read).map_err(err)
}

#[tauri::command]
fn set_saved(state: State<AppState>, id: i64, is_saved: bool) -> Result<Article, String> {
    state.set_saved(id, is_saved).map_err(err)
}

#[tauri::command]
fn mark_all_read(state: State<AppState>, filter: ArticleFilter) -> Result<u64, String> {
    state.mark_all_read(filter).map_err(err)
}

#[tauri::command]
async fn refresh_feed(state: State<'_, AppState>, id: i64) -> Result<RefreshResult, String> {
    state.refresh_feed(id).await.map_err(err)
}

#[tauri::command]
async fn refresh_all(
    state: State<'_, AppState>,
    folder_id: Option<i64>,
) -> Result<RefreshAllResult, String> {
    state.refresh_many(folder_id).await.map_err(err)
}

#[tauri::command]
fn import_opml(state: State<AppState>, xml: String) -> Result<ImportResult, String> {
    state.import_opml(&xml).map_err(err)
}

#[tauri::command]
fn export_opml(state: State<AppState>) -> Result<String, String> {
    state.export_opml().map_err(err)
}

#[tauri::command]
fn list_settings(state: State<AppState>) -> Result<Vec<Setting>, String> {
    state.list_settings().map_err(err)
}

#[tauri::command]
fn set_setting(state: State<AppState>, key: String, value: String) -> Result<(), String> {
    state.set_setting(&key, &value).map_err(err)
}

/// Built-in AI prompts (EN + TR) so the Settings UI can show and reset them.
#[tauri::command]
fn prompt_defaults() -> Vec<crate::prompts::PromptDefault> {
    crate::prompts::defaults()
}

#[tauri::command]
fn probe_ai(state: State<AppState>) -> Result<AiProbe, String> {
    state.probe_ai().map_err(err)
}


#[tauri::command]
async fn list_subscriptions() -> Result<Vec<crate::subscriptions::SubscriptionStatus>, String> {
    Ok(crate::subscriptions::statuses().await)
}

#[tauri::command]
fn open_subscription_login(id: String) -> Result<String, String> {
    crate::subscriptions::open_login(&id).map_err(err)
}

#[tauri::command]
async fn load_full_article(state: State<'_, AppState>, article_id: i64) -> Result<Article, String> {
    state.load_full_article(article_id).await.map_err(err)
}

#[tauri::command]
async fn ai_summarize(state: State<'_, AppState>, article_id: i64) -> Result<AiOutput, String> {
    state.summarize(article_id).await.map_err(err)
}

#[tauri::command]
async fn ai_tags(state: State<'_, AppState>, article_id: i64) -> Result<AiOutput, String> {
    state.tags(article_id).await.map_err(err)
}

#[tauri::command]
async fn ai_brief(state: State<'_, AppState>, folder_id: i64) -> Result<AiOutput, String> {
    state.brief(folder_id).await.map_err(err)
}

#[tauri::command]
fn list_notes(state: State<AppState>) -> Result<Vec<Note>, String> {
    state.list_notes().map_err(err)
}

#[tauri::command]
fn get_note(state: State<AppState>, id: i64) -> Result<Note, String> {
    state.get_note(id).map_err(err)
}

#[tauri::command]
fn create_note(state: State<AppState>, req: CreateNoteRequest) -> Result<Note, String> {
    state.create_note(req).map_err(err)
}

#[tauri::command]
fn update_note(state: State<AppState>, id: i64, req: UpdateNoteRequest) -> Result<Note, String> {
    state.update_note(id, req).map_err(err)
}

#[tauri::command]
fn delete_note(state: State<AppState>, id: i64) -> Result<(), String> {
    state.delete_note(id).map_err(err)
}

#[tauri::command]
fn list_prompt_templates(state: State<AppState>) -> Result<Vec<PromptTemplate>, String> {
    state.list_prompt_templates().map_err(err)
}

#[tauri::command]
fn create_prompt_template(
    state: State<AppState>,
    req: CreatePromptTemplateRequest,
) -> Result<PromptTemplate, String> {
    state.create_prompt_template(req).map_err(err)
}

#[tauri::command]
fn update_prompt_template(
    state: State<AppState>,
    id: i64,
    req: UpdatePromptTemplateRequest,
) -> Result<PromptTemplate, String> {
    state.update_prompt_template(id, req).map_err(err)
}

#[tauri::command]
fn delete_prompt_template(state: State<AppState>, id: i64) -> Result<(), String> {
    state.delete_prompt_template(id).map_err(err)
}

#[tauri::command]
async fn ai_summarize_table(state: State<'_, AppState>, article_id: i64) -> Result<AiOutput, String> {
    state.summarize_table(article_id).await.map_err(err)
}

/// Write AI export bytes to user-authorized Downloads (sanitized filename).
/// First export may show a one-time folder picker; path is remembered in settings
/// so later writes do not trigger repeated macOS Downloads TCC prompts.
#[tauri::command]
fn save_ai_export(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    filename: String,
    bytes_base64: String,
) -> Result<String, String> {
    use base64::Engine;
    use std::path::PathBuf;
    use tauri_plugin_dialog::DialogExt;

    const SETTING_KEY: &str = "export_downloads_dir";

    let safe = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let safe = safe.trim_matches('.').trim_matches('_');
    let safe = if safe.is_empty() {
        "feedcairn-ai-export".to_string()
    } else {
        safe.to_string()
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(bytes_base64.trim())
        .map_err(|e| format!("base64 decode: {e}"))?;

    let downloads_fallback = dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Downloads")));

    let stored: Option<String> = state
        .with_db(|c| crate::db::get_setting(c, SETTING_KEY))
        .map_err(err)?;

    let mut dir: Option<PathBuf> = stored
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_dir());

    // Probe write access on remembered path; if it fails, re-prompt.
    if let Some(ref d) = dir {
        let probe = d.join(".feedcairn-write-probe");
        match std::fs::write(&probe, b"ok") {
            Ok(()) => {
                let _ = std::fs::remove_file(&probe);
            }
            Err(_) => {
                dir = None;
            }
        }
    }

    if dir.is_none() {
        let mut picker = app
            .dialog()
            .file()
            .set_title("İndirilenler klasörünü seçin / Choose a download folder");
        if let Some(ref dl) = downloads_fallback {
            picker = picker.set_directory(dl);
        }
        let picked = picker.blocking_pick_folder().ok_or_else(|| {
            "İndirme iptal edildi — bir indirme klasörü seçmeniz gerekiyor / Download cancelled — please choose a download folder".to_string()
        })?;
        let chosen_path = picked
            .into_path()
            .map_err(|e| format!("klasör yolu / folder path: {e}"))?;
        if !chosen_path.is_dir() {
            return Err("Seçilen yol bir klasör değil / The chosen path is not a folder".to_string());
        }
        state
            .set_setting(SETTING_KEY, &chosen_path.to_string_lossy())
            .map_err(err)?;
        dir = Some(chosen_path);
    }

    let dir = dir.ok_or_else(|| "Kayıt klasörü bulunamadı / Save folder not found".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let mut path = dir.join(&safe);
    if path.exists() {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("feedcairn-ai");
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|e| format!(".{e}"))
            .unwrap_or_default();
        for i in 1..1000 {
            let candidate = dir.join(format!("{stem}-{i}{ext}"));
            if !candidate.exists() {
                path = candidate;
                break;
            }
        }
    }
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    let _ = tauri_plugin_opener::reveal_item_in_dir(&path);
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
async fn ask_ai(state: State<'_, AppState>, req: AskAiRequest) -> Result<AskAiResponse, String> {
    state.ask_ai(req).await.map_err(err)
}

pub fn run_tauri() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&dir)?;
            let db_path = if let Ok(p) = std::env::var("FEEDCAIRN_DB") {
                std::path::PathBuf::from(p)
            } else {
                dir.join("feedcairn.db")
            };
            let state = AppState::open(db_path).map_err(|e| e.to_string())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            create_folder,
            rename_folder,
            delete_folder,
            add_feed,
            update_feed,
            delete_feed,
            list_articles,
            get_article,
            set_read,
            set_saved,
            mark_all_read,
            refresh_feed,
            refresh_all,
            import_opml,
            export_opml,
            list_settings,
            set_setting,
            probe_ai,
            prompt_defaults,
            list_subscriptions,
            open_subscription_login,
            load_full_article,
            ai_summarize,
            ai_summarize_table,
            ai_tags,
            ai_brief,
            list_notes,
            get_note,
            create_note,
            update_note,
            delete_note,
            list_prompt_templates,
            create_prompt_template,
            update_prompt_template,
            delete_prompt_template,
            ask_ai,
            save_ai_export
        ])
        .run(tauri::generate_context!())
        .expect("error while running Feedcairn");
}
