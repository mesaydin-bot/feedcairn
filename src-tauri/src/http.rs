use crate::app::AppState;
use crate::error::Error;
use crate::models::*;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;

/// Header carrying the per-launch session token of the optional web/API mode.
pub const TOKEN_HEADER: &str = "x-feedcairn-token";

/// Guard for the loopback API used by `npm run web` (the desktop app uses Tauri IPC
/// and never opens this port). Modeled on Polyquill's server:
/// Host allow-list (DNS rebinding), loopback-only Origin, no cross-site requests,
/// per-launch random token, JSON-only bodies. No CORS headers are sent at all.
#[derive(Clone)]
pub struct ApiGuard {
    pub token: Arc<String>,
    pub port: u16,
}

pub fn new_session_token() -> String {
    if let Ok(t) = std::env::var("FEEDCAIRN_TOKEN") {
        if t.len() >= 32 && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return t;
        }
    }
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("OS random source");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn loopback_origin(origin: &str) -> bool {
    match url::Url::parse(origin) {
        Ok(u) => {
            matches!(u.scheme(), "http" | "https")
                && matches!(u.host_str(), Some("127.0.0.1") | Some("localhost") | Some("[::1]"))
        }
        Err(_) => false,
    }
}

fn refuse(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

pub fn check_request(guard: &ApiGuard, req: &Request) -> Option<Response> {
    let headers = req.headers();
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
    let allowed_hosts = [format!("127.0.0.1:{}", guard.port), format!("localhost:{}", guard.port)];
    if !allowed_hosts.iter().any(|h| h == host) {
        return Some(refuse(StatusCode::FORBIDDEN, "Only local loopback connections are allowed."));
    }
    match headers.get(header::ORIGIN).map(|v| v.to_str().unwrap_or("")) {
        Some(origin) if !loopback_origin(origin) => {
            return Some(refuse(StatusCode::FORBIDDEN, "Origin refused."));
        }
        None if headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) == Some("cross-site") => {
            return Some(refuse(StatusCode::FORBIDDEN, "Cross-site request refused."));
        }
        _ => {}
    }
    let path = req.uri().path();
    if req.method() == Method::GET && (path == "/api/health" || path == "/api/session") {
        return None;
    }
    let presented = headers.get(TOKEN_HEADER).and_then(|v| v.to_str().ok()).unwrap_or("");
    if !constant_time_eq(presented.as_bytes(), guard.token.as_bytes()) {
        return Some(refuse(StatusCode::UNAUTHORIZED, "Local session token required. Reload the page."));
    }
    if matches!(*req.method(), Method::POST | Method::PUT | Method::PATCH) {
        if let Some(ct) = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()) {
            if !ct.to_ascii_lowercase().starts_with("application/json") {
                return Some(refuse(StatusCode::UNSUPPORTED_MEDIA_TYPE, "application/json required."));
            }
        }
    }
    None
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn guard_layer(State(guard): State<ApiGuard>, req: Request, next: Next) -> Response {
    if let Some(refused) = check_request(&guard, &req) {
        return refused;
    }
    next.run(req).await
}

/// Settings that choose which executable runs (and with which arguments) can only be
/// changed in the desktop app, never over HTTP.
pub fn http_settable(key: &str) -> bool {
    !(key.starts_with("ai_cmd_") || key.starts_with("ai_args_"))
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = match &self {
            Error::CliMissing { .. } => StatusCode::FAILED_DEPENDENCY,
            Error::Message(m) if m.to_lowercase().contains("not found") => StatusCode::NOT_FOUND,
            _ => StatusCode::BAD_REQUEST,
        };
        (status, Json(self)).into_response()
    }
}

pub async fn router(state: AppState, guard: ApiGuard) -> Router {
    let session_token = guard.token.clone();
    Router::new()
        .route("/api/health", get(health))
        .route(
            "/api/session",
            get(move || {
                let token = session_token.clone();
                async move { Json(serde_json::json!({ "token": token.as_str() })) }
            }),
        )
        .route("/api/subscriptions", get(list_subscriptions))
        .route("/api/subscriptions/{id}/login", post(open_subscription_login))
        .route("/api/bootstrap", get(bootstrap))
        .route("/api/folders", post(create_folder))
        .route("/api/folders/{id}/rename", post(rename_folder))
        .route("/api/folders/{id}", axum::routing::delete(delete_folder))
        .route("/api/feeds", post(add_feed))
        .route("/api/feeds/{id}", axum::routing::patch(update_feed).delete(delete_feed))
        .route("/api/articles", get(list_articles))
        .route("/api/articles/{id}", get(get_article))
        .route("/api/articles/{id}/read", post(set_read))
        .route("/api/articles/{id}/save", post(set_saved))
        .route("/api/articles/{id}/full", post(load_full_article))
        .route("/api/articles/mark-all-read", post(mark_all_read))
        .route("/api/refresh", post(refresh_all))
        .route("/api/refresh/{id}", post(refresh_feed))
        .route("/api/opml/import", post(import_opml))
        .route("/api/opml/export", get(export_opml))
        .route("/api/settings", get(list_settings).put(put_settings))
        .route("/api/ai/probe", get(probe_ai))
        .route("/api/prompts/defaults", get(prompt_defaults))
        .route("/api/ai/summarize/{id}", post(ai_summarize))
        .route("/api/ai/summarize-table/{id}", post(ai_summarize_table))
        .route("/api/ai/tags/{id}", post(ai_tags))
        .route("/api/ai/brief/{id}", post(ai_brief))
        .route("/api/ai/ask", post(ask_ai))
        .route("/api/notes", get(list_notes).post(create_note))
        .route(
            "/api/notes/{id}",
            get(get_note)
                .patch(update_note)
                .delete(delete_note),
        )
        .route(
            "/api/prompt-templates",
            get(list_prompt_templates).post(create_prompt_template),
        )
        .route(
            "/api/prompt-templates/{id}",
            axum::routing::patch(update_prompt_template).delete(delete_prompt_template),
        )
        .layer(middleware::from_fn_with_state(guard, guard_layer))
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true, "name": "Feedcairn" }))
}

async fn bootstrap(State(state): State<AppState>) -> Result<Json<Bootstrap>, Error> {
    Ok(Json(state.bootstrap()?))
}

async fn create_folder(
    State(state): State<AppState>,
    Json(req): Json<CreateFolderRequest>,
) -> Result<Json<Folder>, Error> {
    Ok(Json(state.create_folder(req)?))
}

#[derive(Deserialize)]
struct Rename {
    name: String,
}

async fn rename_folder(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<Rename>,
) -> Result<Json<serde_json::Value>, Error> {
    state.rename_folder(id, &req.name)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn delete_folder(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, Error> {
    state.delete_folder(id)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn add_feed(
    State(state): State<AppState>,
    Json(req): Json<AddFeedRequest>,
) -> Result<Json<Feed>, Error> {
    Ok(Json(state.add_feed_and_fetch(req).await?))
}

async fn update_feed(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateFeedRequest>,
) -> Result<Json<Feed>, Error> {
    Ok(Json(state.update_feed(id, req)?))
}

async fn delete_feed(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, Error> {
    state.delete_feed(id)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn list_articles(
    State(state): State<AppState>,
    Query(filter): Query<ArticleFilter>,
) -> Result<Json<Vec<ArticleListItem>>, Error> {
    Ok(Json(state.list_articles(filter)?))
}

async fn get_article(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Article>, Error> {
    Ok(Json(state.get_article(id)?))
}

#[derive(Deserialize)]
struct Flag {
    value: bool,
}

async fn set_read(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(flag): Json<Flag>,
) -> Result<Json<Article>, Error> {
    Ok(Json(state.set_read(id, flag.value)?))
}

async fn set_saved(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(flag): Json<Flag>,
) -> Result<Json<Article>, Error> {
    Ok(Json(state.set_saved(id, flag.value)?))
}

async fn mark_all_read(
    State(state): State<AppState>,
    Json(filter): Json<ArticleFilter>,
) -> Result<Json<serde_json::Value>, Error> {
    let n = state.mark_all_read(filter)?;
    Ok(Json(serde_json::json!({ "updated": n })))
}

async fn refresh_feed(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<RefreshResult>, Error> {
    Ok(Json(state.refresh_feed(id).await?))
}

#[derive(Deserialize, Default)]
struct RefreshQuery {
    folder_id: Option<i64>,
}

async fn refresh_all(
    State(state): State<AppState>,
    Query(q): Query<RefreshQuery>,
) -> Result<Json<RefreshAllResult>, Error> {
    Ok(Json(state.refresh_many(q.folder_id).await?))
}

#[derive(Deserialize)]
struct OpmlBody {
    xml: String,
}

async fn import_opml(
    State(state): State<AppState>,
    Json(body): Json<OpmlBody>,
) -> Result<Json<ImportResult>, Error> {
    Ok(Json(state.import_opml(&body.xml)?))
}

async fn export_opml(State(state): State<AppState>) -> Result<impl IntoResponse, Error> {
    let xml = state.export_opml()?;
    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "text/xml; charset=utf-8",
        )],
        xml,
    ))
}

async fn list_settings(State(state): State<AppState>) -> Result<Json<Vec<Setting>>, Error> {
    Ok(Json(state.list_settings()?))
}

#[derive(Deserialize)]
struct SettingsBody {
    settings: Vec<Setting>,
}

async fn put_settings(
    State(state): State<AppState>,
    Json(body): Json<SettingsBody>,
) -> Result<Json<serde_json::Value>, Error> {
    if let Some(blocked) = body.settings.iter().find(|s| !http_settable(&s.key)) {
        return Err(Error::msg(format!(
            "`{}` can only be changed in the desktop app.",
            blocked.key
        )));
    }
    for s in body.settings {
        state.set_setting(&s.key, &s.value)?;
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn probe_ai(State(state): State<AppState>) -> Result<Json<AiProbe>, Error> {
    Ok(Json(state.probe_ai()?))
}

async fn prompt_defaults() -> Json<Vec<crate::prompts::PromptDefault>> {
    Json(crate::prompts::defaults())
}


async fn load_full_article(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Article>, Error> {
    Ok(Json(state.load_full_article(id).await?))
}

async fn ai_summarize(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<AiOutput>, Error> {
    Ok(Json(state.summarize(id).await?))
}

async fn ai_summarize_table(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<AiOutput>, Error> {
    Ok(Json(state.summarize_table(id).await?))
}

async fn ai_tags(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<AiOutput>, Error> {
    Ok(Json(state.tags(id).await?))
}

async fn ai_brief(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<AiOutput>, Error> {
    Ok(Json(state.brief(id).await?))
}


async fn list_notes(State(state): State<AppState>) -> Result<Json<Vec<Note>>, Error> {
    Ok(Json(state.list_notes()?))
}

async fn get_note(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Note>, Error> {
    Ok(Json(state.get_note(id)?))
}

async fn create_note(
    State(state): State<AppState>,
    Json(req): Json<CreateNoteRequest>,
) -> Result<Json<Note>, Error> {
    Ok(Json(state.create_note(req)?))
}

async fn update_note(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateNoteRequest>,
) -> Result<Json<Note>, Error> {
    Ok(Json(state.update_note(id, req)?))
}

async fn delete_note(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, Error> {
    state.delete_note(id)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}


async fn list_prompt_templates(
    State(state): State<AppState>,
) -> Result<Json<Vec<PromptTemplate>>, Error> {
    Ok(Json(state.list_prompt_templates()?))
}

async fn create_prompt_template(
    State(state): State<AppState>,
    Json(req): Json<CreatePromptTemplateRequest>,
) -> Result<Json<PromptTemplate>, Error> {
    Ok(Json(state.create_prompt_template(req)?))
}

async fn update_prompt_template(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdatePromptTemplateRequest>,
) -> Result<Json<PromptTemplate>, Error> {
    Ok(Json(state.update_prompt_template(id, req)?))
}

async fn delete_prompt_template(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, Error> {
    state.delete_prompt_template(id)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn ask_ai(
    State(state): State<AppState>,
    Json(req): Json<AskAiRequest>,
) -> Result<Json<AskAiResponse>, Error> {
    Ok(Json(state.ask_ai(req).await?))
}

async fn list_subscriptions() -> Json<Vec<crate::subscriptions::SubscriptionStatus>> {
    Json(crate::subscriptions::statuses().await)
}

async fn open_subscription_login(Path(id): Path<String>) -> Result<Json<serde_json::Value>, Error> {
    let message = crate::subscriptions::open_login(&id)?;
    Ok(Json(serde_json::json!({ "ok": true, "message": message })))
}

pub async fn serve(state: AppState, addr: SocketAddr) -> Result<(), Error> {
    if !addr.ip().is_loopback() {
        return Err(Error::msg("the API only binds to a loopback address"));
    }
    let guard = ApiGuard { token: Arc::new(new_session_token()), port: addr.port() };
    let app = router(state, guard).await;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("Feedcairn API listening on http://{addr} (token-protected, loopback only)");
    axum::serve(listener, app)
        .await
        .map_err(|e| Error::msg(e.to_string()))
}

pub fn run_web_blocking() {
    let db_path = crate::app::default_db_path();
    let state = AppState::open(db_path).expect("open database");
    let port: u16 = std::env::var("FEEDCAIRN_API_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(43124);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        serve(state, addr).await.expect("api server");
    });
}

#[cfg(test)]
mod guard_tests {
    use super::*;
    use axum::body::Body;

    fn guard() -> ApiGuard {
        ApiGuard { token: Arc::new("t".repeat(40)), port: 43124 }
    }

    fn req(method: &str, path: &str, headers: &[(&str, &str)]) -> Request {
        let mut b = axum::http::Request::builder().method(method).uri(path);
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        b.body(Body::empty()).unwrap()
    }

    fn status(r: Option<Response>) -> Option<StatusCode> {
        r.map(|r| r.status())
    }

    #[test]
    fn session_is_open_on_loopback() {
        let r = req("GET", "/api/session", &[("host", "127.0.0.1:43124")]);
        assert_eq!(status(check_request(&guard(), &r)), None);
    }

    #[test]
    fn rebinding_host_is_refused() {
        let r = req("GET", "/api/session", &[("host", "evil.example:43124")]);
        assert_eq!(status(check_request(&guard(), &r)), Some(StatusCode::FORBIDDEN));
    }

    #[test]
    fn foreign_origin_is_refused() {
        let tok = "t".repeat(40);
        let r = req(
            "GET",
            "/api/feeds",
            &[("host", "127.0.0.1:43124"), ("origin", "https://evil.example"), (TOKEN_HEADER, &tok)],
        );
        assert_eq!(status(check_request(&guard(), &r)), Some(StatusCode::FORBIDDEN));
    }

    #[test]
    fn token_required_and_json_enforced() {
        let r = req("GET", "/api/feeds", &[("host", "localhost:43124")]);
        assert_eq!(status(check_request(&guard(), &r)), Some(StatusCode::UNAUTHORIZED));
        let tok = "t".repeat(40);
        let ok = req("GET", "/api/feeds", &[("host", "localhost:43124"), ("origin", "http://localhost:5173"), (TOKEN_HEADER, &tok)]);
        assert_eq!(status(check_request(&guard(), &ok)), None);
        let text = req(
            "POST",
            "/api/feeds",
            &[("host", "localhost:43124"), (TOKEN_HEADER, &tok), ("content-type", "text/plain")],
        );
        assert_eq!(status(check_request(&guard(), &text)), Some(StatusCode::UNSUPPORTED_MEDIA_TYPE));
    }

    #[test]
    fn executable_settings_not_http_settable() {
        assert!(!http_settable("ai_cmd_kimi"));
        assert!(!http_settable("ai_args_codex"));
    }
}
