//! AI provider: spawn a user-owned CLI, wait with a timeout, return stdout.
//! API keys never leave the user's machine and are never stored in this repo.
//!
//! Session reuse: the first successful CLI call per provider (for the lifetime of
//! this Feedcairn process) opens a chat; later Summarize / Ask AI / tag / brief
//! calls resume that same chat via in-memory session ids. Nothing is persisted
//! in SQLite — closing the app clears the store.

use crate::db;
use crate::error::{Error, Result};
use crate::models::{AiOutput, AiProbe, Article, ArticleListItem};
use once_cell::sync::Lazy;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::timeout;

const AI_TIMEOUT_SECS: u64 = 120;

/// In-process session ids keyed by `provider:command`. Cleared when the process exits.
static AI_SESSIONS: Lazy<Mutex<HashMap<String, Option<String>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Canonical providers plus the binaries we try, in order, when Settings has no override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderId {
    ChatGpt,
    Kimi,
    Cursor,
    Antigravity,
    /// Claude Code CLI (`claude`), Claude Pro/Max/Team/Enterprise subscription login only.
    Claude,
}

impl ProviderId {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "openai" | "openai-compatible" | "chatgpt" | "codex" => Self::ChatGpt,
            "kimi" => Self::Kimi,
            "cursor" => Self::Cursor,
            "agy" | "antigravity" => Self::Antigravity,
            "claude" | "claude-code" => Self::Claude,
            _ => Self::ChatGpt,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::ChatGpt => "chatgpt",
            Self::Kimi => "kimi",
            Self::Cursor => "cursor",
            Self::Antigravity => "antigravity",
            Self::Claude => "claude",
        }
    }

    /// Binaries probed on PATH when the settings command is empty or missing.
    pub fn binaries(self) -> &'static [&'static str] {
        match self {
            // ChatGPT subscription CLI is Codex. The metered `openai` API CLI is never used.
            Self::ChatGpt => &["codex", "chatgpt"],
            Self::Kimi => &["kimi"],
            Self::Cursor => &["cursor-agent", "cursor"],
            Self::Antigravity => &["antigravity", "agy"],
            Self::Claude => &["claude"],
        }
    }
}

/// Normalises the Cursor model setting to a plain model id (see Tabulect `normalizeModelId`).
pub fn cursor_model_id(raw: &str) -> String {
    let mut out = String::new();
    let mut depth = 0u32;
    for c in raw.trim().chars() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            c if depth == 0 && !c.is_whitespace() => out.push(c),
            _ => {}
        }
    }
    let ok = !out.is_empty()
        && out.len() <= 80
        && out.chars().next().map_or(false, |c| c.is_ascii_alphanumeric())
        && out.chars().all(|c| c.is_ascii_alphanumeric() || "._:+/-".contains(c));
    if !ok || out == "composer-1" || out == "default" {
        "auto".into()
    } else {
        out
    }
}

/// Dedicated cwd so Codex/Kimi resume/--last/--continue stay Feedcairn-scoped.
pub fn ai_session_workdir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("org.thinkingtoolslab.feedcairn")
        .join("ai-session")
}

fn ensure_ai_session_workdir() -> Result<PathBuf> {
    let dir = ai_session_workdir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn session_map_key(id: ProviderId, command: &str) -> String {
    format!("{}:{}", id.key(), command)
}

/// Resolved CLI invocation. Spawn + timeout live on [`AiProvider::complete`].
#[derive(Debug, Clone)]
pub struct AiProvider {
    pub id: ProviderId,
    pub command: String,
    pub args: Vec<String>,
}

impl AiProvider {
    pub fn from_settings(settings: &HashMap<String, String>) -> Self {
        let id = ProviderId::parse(&provider_from(settings));
        let command = resolve_command(settings, id);
        let args = build_args(settings, id, &command);
        Self { id, command, args }
    }

    pub fn probe(&self) -> AiProbe {
        let found = which(&self.command);
        let key = self.id.key();
        AiProbe {
            hint: if found {
                format!("Found `{}` on PATH (provider {key}).", crate::models::display_command(&self.command))
            } else {
                install_hint(key, &self.command)
            },
            provider: key.to_string(),
            command: self.command.clone(),
            found,
        }
    }

    /// Whether this provider/command pair supports in-process CLI session reuse.
    fn session_capable(&self) -> bool {
        match self.id {
            ProviderId::ChatGpt => is_codex_command(&self.command),
            ProviderId::Kimi => true,
            ProviderId::Cursor => {
                let c = self.command.as_str();
                c == "cursor" || c.ends_with("/cursor") || c.ends_with("\\cursor")
            }
            ProviderId::Antigravity => false,
            ProviderId::Claude => is_claude_command(&self.command),
        }
    }

    pub async fn complete(&self, prompt: &str) -> Result<AiOutput> {
        if self.session_capable() {
            return self.complete_with_session(prompt).await;
        }
        self.complete_oneshot(prompt).await
    }

    async fn complete_with_session(&self, prompt: &str) -> Result<AiOutput> {
        let key = session_map_key(self.id, &self.command);
        // Only hold the mutex while reading/writing the session id — never during CLI wait.
        // Keeps the shared session across asks until process exit, without serializing concurrent asks.
        let existing = {
            let sessions = AI_SESSIONS.lock().await;
            sessions.get(&key).cloned().flatten()
        };

        match self
            .run_session_attempt(prompt, existing.as_deref())
            .await
        {
            Ok((out, new_id)) => {
                let mut sessions = AI_SESSIONS.lock().await;
                sessions.insert(key, new_id.or(existing));
                Ok(out)
            }
            Err(_err) if existing.is_some() => {
                // Session gone or resume failed — clear and open a fresh chat once.
                {
                    let mut sessions = AI_SESSIONS.lock().await;
                    sessions.insert(key.clone(), None);
                }
                match self.run_session_attempt(prompt, None).await {
                    Ok((out, new_id)) => {
                        let mut sessions = AI_SESSIONS.lock().await;
                        sessions.insert(key, new_id);
                        Ok(out)
                    }
                    Err(err2) => Err(err2),
                }
            }
            Err(err) => Err(err),
        }
    }

    async fn run_session_attempt(
        &self,
        prompt: &str,
        session_id: Option<&str>,
    ) -> Result<(AiOutput, Option<String>)> {
        let workdir = ensure_ai_session_workdir()?;
        let out_file = workdir.join(format!("last-message-{}.txt", std::process::id()));

        let (args, use_stdin) = match self.id {
            ProviderId::ChatGpt if is_codex_command(&self.command) => {
                let args = build_codex_invocation(
                    session_id,
                    &self.args,
                    &workdir,
                    &out_file,
                );
                (args, true)
            }
            ProviderId::Kimi => {
                let args = build_kimi_invocation(session_id, &self.args, &workdir);
                (args, true)
            }
            ProviderId::Claude if is_claude_command(&self.command) => {
                (build_claude_invocation(session_id, &self.args), true)
            }
            ProviderId::Cursor => {
                // Prompt already substituted into argv; do not also write stdin.
                let args = build_cursor_invocation(session_id.is_some(), &self.args, prompt);
                (args, false)
            }
            _ => {
                return self
                    .complete_oneshot(prompt)
                    .await
                    .map(|o| (o, None));
            }
        };

        let raw = run_cli_raw(
            self.id.key(),
            &self.command,
            &args,
            if use_stdin { Some(prompt) } else { None },
            Some(&workdir),
        )
        .await?;
        let body = finalize_cli_body(self.id, &self.command, &raw.stdout, &raw.stderr, &out_file)?;
        let new_id = match self.id {
            ProviderId::ChatGpt if is_codex_command(&self.command) => {
                extract_codex_thread_id(&raw.stdout)
                    .or_else(|| session_id.map(|s| s.to_string()))
                    // cwd-isolated --last fallback if JSONL lacked thread.started
                    .or_else(|| Some(String::from("__last__")))
            }
            ProviderId::Kimi => extract_kimi_session_id(&raw.stdout, &raw.stderr)
                .or_else(|| session_id.map(|s| s.to_string()))
                // Mark established so later calls use --continue even without a parsed id.
                .or_else(|| Some(String::from("__continue__"))),
            ProviderId::Cursor => session_id
                .map(|s| s.to_string())
                .or_else(|| extract_cursor_chat_id(&raw.stdout, &raw.stderr))
                .or_else(|| Some(String::from("__continue__"))),
            ProviderId::Claude => extract_claude_session_id(&raw.stdout)
                .or_else(|| session_id.map(|s| s.to_string())),
            _ => None,
        };

        let _ = std::fs::remove_file(&out_file);

        Ok((
            AiOutput {
                provider: self.id.key().to_string(),
                command: self.command.clone(),
                output: body,
            },
            new_id,
        ))
    }

    async fn complete_oneshot(&self, prompt: &str) -> Result<AiOutput> {
        // Cursor's `--workspace` points at this folder; make sure it exists.
        ensure_ai_session_workdir()?;
        let provider = self.id.key().to_string();
        let command = self.command.clone();
        let mut use_stdin = true;
        let args: Vec<String> = self
            .args
            .iter()
            .map(|a| {
                if a.contains("{prompt}") {
                    use_stdin = false;
                    a.replace("{prompt}", prompt)
                } else {
                    a.clone()
                }
            })
            .collect();

        let raw = run_cli_raw(
            &provider,
            &command,
            &args,
            if use_stdin { Some(prompt) } else { None },
            None,
        )
        .await?;
        let mut body = if raw.stdout.is_empty() {
            raw.stderr.clone()
        } else {
            raw.stdout.clone()
        };
        Ok(AiOutput {
            provider,
            command,
            output: body,
        })
    }
}

/// Environment variables that would switch a CLI from the user's own subscription
/// login to metered API billing (or leak a token into a child process).
pub fn scrubbed_env_keys() -> Vec<String> {
    const EXACT: &[&str] = &[
        "OPENAI_API_KEY", "OPENAI_BASE_URL", "OPENAI_ORG_ID", "ANTHROPIC_API_KEY",
        "GEMINI_API_KEY", "GOOGLE_API_KEY", "GOOGLE_GENAI_USE_VERTEXAI", "GOOGLE_GENAI_USE_GCA",
        "GOOGLE_GEMINI_BASE_URL", "KIMI_API_KEY", "KIMI_BASE_URL", "MOONSHOT_API_KEY",
        "CURSOR_API_KEY", "CURSOR_AUTH_TOKEN", "CODEX_THREAD_ID", "FEEDCAIRN_TOKEN",
        // Claude Code: API keys, bearer/OAuth tokens and cloud-provider switches all take
        // precedence over the claude.ai subscription login, so none may reach the CLI.
        "ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_BASE_URL", "ANTHROPIC_PROFILE",
        "ANTHROPIC_FEDERATION_RULE_ID", "ANTHROPIC_ORGANIZATION_ID", "ANTHROPIC_IDENTITY_TOKEN_FILE",
        "CLAUDE_CODE_OAUTH_TOKEN", "CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX",
        "CLAUDE_CODE_USE_FOUNDRY",
    ];
    let mut keys: Vec<String> = EXACT.iter().map(|k| k.to_string()).collect();
    for (key, _) in std::env::vars_os() {
        if let Some(k) = key.to_str() {
            if k.starts_with("KIMI_MODEL_") || k.starts_with("CODEX_INTERNAL_") {
                keys.push(k.to_string());
            }
        }
    }
    keys
}

struct RawCli {
    stdout: String,
    stderr: String,
}

async fn run_cli_raw(
    provider: &str,
    command: &str,
    args: &[String],
    stdin_prompt: Option<&str>,
    current_dir: Option<&Path>,
) -> Result<RawCli> {
    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .env("PATH", augmented_path_env());
    if provider == "cursor" {
        // Headless Cursor Agent: no TTY prompts or colour codes.
        cmd.env("TERM", "dumb").env("CI", "1").env("NO_COLOR", "1");
    }
    // Subscription mode only: never fall back to metered API keys from the environment.
    for key in scrubbed_env_keys() {
        cmd.env_remove(key);
    }
    if let Some(dir) = current_dir {
        cmd.current_dir(dir);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::CliMissing {
                hint: install_hint(provider, command),
                provider: provider.to_string(),
                command: command.to_string(),
            });
        }
        Err(e) => return Err(e.into()),
    };

    if let Some(prompt) = stdin_prompt {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(prompt.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
        }
    } else {
        // Drop stdin so CLIs that wait on it don't hang.
        drop(child.stdin.take());
    }

    let output = timeout(Duration::from_secs(AI_TIMEOUT_SECS), child.wait_with_output())
        .await
        .map_err(|_| Error::AiTimeout(AI_TIMEOUT_SECS))??;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        let detail = if stderr.is_empty() {
            stdout
        } else {
            stderr
        };
        return Err(Error::AiFailed(if detail.is_empty() {
            format!("exit {}", output.status)
        } else {
            detail
        }));
    }
    Ok(RawCli { stdout, stderr })
}

fn finalize_cli_body(
    id: ProviderId,
    command: &str,
    stdout: &str,
    stderr: &str,
    out_file: &Path,
) -> Result<String> {
    if let Ok(text) = std::fs::read_to_string(out_file) {
        let t = text.trim().to_string();
        if !t.is_empty() {
            return Ok(t);
        }
    }
    if id == ProviderId::ChatGpt && is_codex_command(command) {
        if let Some(msg) = extract_codex_last_message(stdout) {
            return Ok(msg);
        }
    }
    if id == ProviderId::Claude && is_claude_command(command) {
        return extract_claude_result(stdout);
    }
    let body = if stdout.is_empty() {
        stderr.to_string()
    } else {
        stdout.to_string()
    };
    Ok(body)
}

fn is_claude_command(command: &str) -> bool {
    command == "claude" || command.ends_with("/claude") || command.ends_with("\\claude")
}

/// `claude -p` print mode, text only: no built-in tools, no MCP servers, no slash commands,
/// never asks for permissions. The prompt arrives on stdin. Verified against
/// https://code.claude.com/docs/en/cli-reference.
fn claude_print_args(model: &str, effort: &str) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p", "--output-format", "json", "--tools", "", "--strict-mcp-config",
        "--disable-slash-commands", "--permission-mode", "dontAsk",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if !model.is_empty() && model != "default" {
        args.push("--model".into());
        args.push(model.into());
    }
    if !effort.is_empty() && effort != "default" {
        args.push("--effort".into());
        args.push(effort.into());
    }
    args
}

fn build_claude_invocation(session_id: Option<&str>, base_args: &[String]) -> Vec<String> {
    let mut args = base_args.to_vec();
    if let Some(id) = session_id {
        args.push("--resume".into());
        args.push(id.to_string());
    }
    args
}

fn claude_result_json(stdout: &str) -> Option<serde_json::Value> {
    stdout
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .find(|v| v.get("type").and_then(|t| t.as_str()) == Some("result"))
        .or_else(|| serde_json::from_str::<serde_json::Value>(stdout.trim()).ok())
}

fn extract_claude_session_id(stdout: &str) -> Option<String> {
    claude_result_json(stdout)?
        .get("session_id")
        .and_then(|t| t.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// The reply text of a `--output-format json` result; error results are errors, never text.
fn extract_claude_result(stdout: &str) -> Result<String> {
    let Some(v) = claude_result_json(stdout) else {
        return Err(Error::AiFailed("Claude Code returned no JSON result.".into()));
    };
    let is_error = v.get("is_error").and_then(|b| b.as_bool()).unwrap_or(false);
    let text = v.get("result").and_then(|t| t.as_str()).unwrap_or("").trim().to_string();
    if is_error || text.is_empty() {
        let subtype = v.get("subtype").and_then(|t| t.as_str()).unwrap_or("error");
        return Err(Error::AiFailed(if text.is_empty() {
            format!("Claude Code did not produce a reply ({subtype}).")
        } else {
            text
        }));
    }
    Ok(text)
}

fn is_codex_command(command: &str) -> bool {
    command == "codex" || command.ends_with("/codex") || command.ends_with("\\codex")
}

/// Pull model / effort overrides already baked into `build_args` Codex argv.
fn codex_model_effort_from_base(base: &[String]) -> (Option<String>, Option<String>) {
    let mut model = None;
    let mut effort = None;
    let mut i = 0;
    while i < base.len() {
        if base[i] == "-m" {
            if let Some(m) = base.get(i + 1) {
                model = Some(m.clone());
            }
            i += 2;
            continue;
        }
        if base[i] == "-c" {
            if let Some(c) = base.get(i + 1) {
                if c.contains("model_reasoning_effort") {
                    effort = Some(c.clone());
                }
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    (model, effort)
}

fn build_codex_invocation(
    session_id: Option<&str>,
    base_args: &[String],
    workdir: &Path,
    out_file: &Path,
) -> Vec<String> {
    let (model, effort) = codex_model_effort_from_base(base_args);
    let out = out_file.to_string_lossy().to_string();
    let mut args = Vec::new();
    if let Some(id) = session_id {
        // Subsequent calls: resume the same thread (or --last if id was not parsed).
        // codex exec resume <id>|--last --skip-git-repo-check -m … -c … --json -o … -
        args.push("exec".into());
        args.push("resume".into());
        if id == "__last__" {
            args.push("--last".into());
        } else {
            args.push(id.to_string());
        }
        args.push("--skip-git-repo-check".into());
    } else {
        // First call: create a persisted (non-ephemeral) session under Feedcairn cwd.
        // codex exec --skip-git-repo-check -C <workdir> -s read-only -m … -c … --json -o … -
        args.push("exec".into());
        args.push("--skip-git-repo-check".into());
        args.push("-C".into());
        args.push(workdir.to_string_lossy().to_string());
        args.push("-s".into());
        args.push("read-only".into());
    }
    if let Some(m) = model {
        args.push("-m".into());
        args.push(m);
    }
    if let Some(c) = effort {
        args.push("-c".into());
        args.push(c);
    }
    args.push("--json".into());
    args.push("-o".into());
    args.push(out);
    args.push("-".into());
    args
}

fn build_kimi_invocation(
    session_id: Option<&str>,
    base_args: &[String],
    workdir: &Path,
) -> Vec<String> {
    let mut args = base_args.to_vec();
    // Isolate from the user's other Kimi chats.
    args.push("--work-dir".into());
    args.push(workdir.to_string_lossy().to_string());
    if let Some(id) = session_id {
        if id != "__continue__" {
            args.push("--session".into());
            args.push(id.to_string());
        } else {
            args.push("--continue".into());
        }
    }
    args
}

fn build_cursor_invocation(resume: bool, base_args: &[String], prompt: &str) -> Vec<String> {
    let mut args: Vec<String> = base_args
        .iter()
        .map(|a| {
            if a.contains("{prompt}") {
                a.replace("{prompt}", prompt)
            } else {
                a.clone()
            }
        })
        .collect();
    if resume {
        // Insert --continue after -p (print mode) when present.
        if let Some(pos) = args.iter().position(|a| a == "-p") {
            args.insert(pos + 1, "--continue".into());
        } else {
            args.insert(0, "--continue".into());
        }
    }
    args
}

fn extract_codex_thread_id(jsonl: &str) -> Option<String> {
    for line in jsonl.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if ty == "thread.started" {
            if let Some(id) = v.get("thread_id").and_then(|t| t.as_str()) {
                return Some(id.to_string());
            }
        }
        // Fallbacks seen in rollout / older event shapes.
        if let Some(id) = v.get("thread_id").and_then(|t| t.as_str()) {
            if looks_like_uuid(id) {
                return Some(id.to_string());
            }
        }
        if let Some(payload) = v.get("payload") {
            if let Some(id) = payload
                .get("session_id")
                .or_else(|| payload.get("id"))
                .and_then(|t| t.as_str())
            {
                if looks_like_uuid(id) {
                    return Some(id.to_string());
                }
            }
        }
    }
    None
}

fn extract_codex_last_message(jsonl: &str) -> Option<String> {
    let mut last = None;
    for line in jsonl.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        // Common shapes: agent_message / message with text.
        if ty == "agent_message" || ty == "message" {
            if let Some(text) = v
                .get("message")
                .and_then(|m| m.as_str())
                .or_else(|| v.get("text").and_then(|t| t.as_str()))
            {
                let t = text.trim();
                if !t.is_empty() {
                    last = Some(t.to_string());
                }
            }
        }
        if let Some(msg) = v.pointer("/msg/message").and_then(|t| t.as_str()) {
            let t = msg.trim();
            if !t.is_empty() {
                last = Some(t.to_string());
            }
        }
        if let Some(item) = v.get("item") {
            if item.get("type").and_then(|t| t.as_str()) == Some("agent_message") {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    let t = text.trim();
                    if !t.is_empty() {
                        last = Some(t.to_string());
                    }
                }
            }
        }
    }
    last
}

fn extract_kimi_session_id(stdout: &str, stderr: &str) -> Option<String> {
    for text in [stdout, stderr] {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                for key in ["session_id", "session", "id"] {
                    if let Some(id) = v.get(key).and_then(|t| t.as_str()) {
                        if !id.is_empty() && id != "__continue__" {
                            return Some(id.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

fn extract_cursor_chat_id(stdout: &str, stderr: &str) -> Option<String> {
    for text in [stdout, stderr] {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                for key in ["chatId", "chat_id", "session_id", "id"] {
                    if let Some(id) = v.get(key).and_then(|t| t.as_str()) {
                        if !id.is_empty() {
                            return Some(id.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

fn looks_like_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 32 && s.chars().filter(|c| *c == '-').count() >= 4
}

pub fn settings_map(conn: &Connection) -> Result<HashMap<String, String>> {
    Ok(db::list_settings(conn)?
        .into_iter()
        .map(|s| (s.key, s.value))
        .collect())
}

pub fn provider_from(settings: &HashMap<String, String>) -> String {
    settings
        .get("ai_provider")
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "chatgpt".into())
}

fn resolve_command(settings: &HashMap<String, String>, id: ProviderId) -> String {
    let custom = settings
        .get(&format!("ai_cmd_{}", id.key()))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    // The metered OpenAI API CLI is refused even when configured: subscription CLIs only.
    let custom = custom.filter(|cmd| {
        let base = cmd.rsplit(['/', '\\']).next().unwrap_or(cmd);
        base != "openai"
    });
    if let Some(cmd) = custom.as_deref() {
        if let Some(resolved) = resolve_executable(cmd) {
            // Keep absolute ai_cmd_* settings verbatim; bare names → absolute.
            if cmd.contains('/') || cmd.contains('\\') {
                return cmd.to_string();
            }
            return resolved.to_string_lossy().into_owned();
        }
        if !id.binaries().contains(&cmd) {
            return cmd.to_string();
        }
        // Default name is missing from PATH — try aliases (agy, openai, …).
    }
    for bin in id.binaries() {
        if let Some(resolved) = resolve_executable(bin) {
            return resolved.to_string_lossy().into_owned();
        }
    }
    custom
        .or_else(|| id.binaries().first().map(|s| (*s).to_string()))
        .unwrap_or_else(|| id.key().to_string())
}
fn setting<'a>(settings: &'a HashMap<String, String>, key: &str, default: &'a str) -> &'a str {
    settings
        .get(key)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
}

fn legacy_or_custom_args(settings: &HashMap<String, String>, id: ProviderId) -> Option<Vec<String>> {
    let raw = settings.get(&format!("ai_args_{}", id.key()))?.trim();
    if raw.is_empty() || raw == "--quiet" {
        return None;
    }
    Some(
        raw.split_whitespace()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
    )
}

fn build_args(settings: &HashMap<String, String>, id: ProviderId, command: &str) -> Vec<String> {
    if let Some(custom) = legacy_or_custom_args(settings, id) {
        return custom;
    }
    match id {
        ProviderId::ChatGpt => {
            // Codex (ChatGPT subscription). Prompt on stdin via trailing `-`.
            // No --ephemeral: session files persist so `exec resume` can continue
            // the same thread until Feedcairn exits (id kept in-process only).
            if is_codex_command(command) {
                let model = setting(settings, "ai_model_chatgpt", "gpt-6-astra");
                let effort = setting(settings, "ai_effort_chatgpt", "xhigh");
                let work = ai_session_workdir();
                vec![
                    "exec".into(),
                    "--skip-git-repo-check".into(),
                    "-C".into(),
                    work.to_string_lossy().to_string(),
                    "-s".into(),
                    "read-only".into(),
                    "-m".into(),
                    model.into(),
                    "-c".into(),
                    format!("model_reasoning_effort=\"{effort}\""),
                    "--json".into(),
                    "-".into(),
                ]
            } else {
                // Generic chatgpt CLI: send prompt on stdin, no bogus --quiet.
                Vec::new()
            }
        }
        ProviderId::Kimi => {
            let model = setting(settings, "ai_model_kimi", "kimi-code/kimi-for-coding");
            let thinking = setting(settings, "ai_thinking_kimi", "true");
            let mut args = vec![
                "--quiet".into(),
                "--model".into(),
                model.into(),
            ];
            if thinking == "true" || thinking == "1" || thinking == "yes" {
                args.push("--thinking".into());
            } else {
                args.push("--no-thinking".into());
            }
            args
        }
        ProviderId::Cursor => {
            // Same model rule as Tabulect: a plain Cursor model id, "auto" by default (the
            // account's own choice). Effort brackets such as `composer-1[effort=high]` are not
            // model ids and are stripped; the old seeded default `composer-1` maps to `auto`.
            let model_arg = cursor_model_id(setting(settings, "ai_model_cursor", "auto"));
            // Same headless shape as Tabulect: print mode, text output, trust the dedicated
            // Feedcairn workspace up front (otherwise the CLI stops at "Workspace Trust
            // Required"), ask mode so it never edits files, and no force flags.
            let mut args: Vec<String> = Vec::new();
            if crate::models::display_command(command) == "cursor" {
                args.push("agent".into());
            }
            args.extend([
                "-p".into(),
                "--output-format".into(),
                "text".into(),
                "--trust".into(),
                "--workspace".into(),
                ai_session_workdir().to_string_lossy().to_string(),
                "--mode".into(),
                "ask".into(),
                "--model".into(),
                model_arg,
                "{prompt}".into(),
            ]);
            args
        }
        ProviderId::Antigravity => Vec::new(),
        ProviderId::Claude => claude_print_args(
            setting(settings, "ai_model_claude", "default"),
            setting(settings, "ai_effort_claude", "default"),
        ),
    }
}

pub fn command_for(settings: &HashMap<String, String>, provider: &str) -> (String, Vec<String>) {
    let mut merged = settings.clone();
    merged.insert("ai_provider".into(), provider.to_string());
    let p = AiProvider::from_settings(&merged);
    (p.command, p.args)
}

pub fn install_hint(provider: &str, command: &str) -> String {
    let command = crate::models::display_command(command);
    match ProviderId::parse(provider) {
        ProviderId::ChatGpt => format!(
            "`{command}` was not found on PATH. Install Codex (`codex`) for your ChatGPT subscription, or set Settings → ChatGPT command."
        ),
        ProviderId::Kimi => format!(
            "`{command}` was not found on PATH. Install the Kimi CLI so `{command}` is available, or override the command in Settings."
        ),
        ProviderId::Cursor => format!(
            "`{command}` was not found on PATH. Install the Cursor CLI and run `cursor agent login`, or set Settings → Cursor command."
        ),
        ProviderId::Antigravity => format!(
            "`{command}` was not found on PATH. Install Antigravity as `antigravity` or the `agy` alias, or override the command in Settings."
        ),
        ProviderId::Claude => format!(
            "`{command}` was not found on PATH. Install Claude Code (`curl -fsSL https://claude.ai/install.sh | bash`) and run `claude auth login` with your Claude subscription, or set Settings → Claude command."
        ),
    }
}

pub fn probe(settings: &HashMap<String, String>) -> AiProbe {
    AiProvider::from_settings(settings).probe()
}

/// Bin dirs GUI/Dock launches often omit from PATH. Only existing dirs are used.
fn cli_extra_bin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut push = |p: PathBuf| {
        if p.is_dir() && !dirs.iter().any(|d| d == &p) {
            dirs.push(p);
        }
    };
    push(PathBuf::from("/opt/homebrew/bin"));
    push(PathBuf::from("/usr/local/bin"));
    if let Some(home) = dirs::home_dir() {
        push(home.join(".local/bin"));
        push(home.join(".npm-global/bin"));
        push(home.join(".cargo/bin"));
        push(home.join(".cursor/bin"));
        push(home.join(".antigravity/bin"));
    }
    push(PathBuf::from(
        "/Applications/Cursor.app/Contents/Resources/app/bin",
    ));
    push(PathBuf::from(
        "/Applications/Antigravity.app/Contents/Resources/app/bin",
    ));
    dirs
}

/// PATH with Homebrew / local / Cursor / Antigravity bins prepended (for spawn + which).
pub(crate) fn augmented_path_env() -> std::ffi::OsString {
    let mut dirs: Vec<PathBuf> = cli_extra_bin_dirs();
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            if !dirs.iter().any(|x| x == &d) {
                dirs.push(d);
            }
        }
    }
    std::env::join_paths(&dirs).unwrap_or_else(|_| {
        std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into())
    })
}

/// Resolve `cmd` to an absolute path using augmented search dirs + PATH.
pub(crate) fn resolve_executable(cmd: &str) -> Option<PathBuf> {
    if cmd.contains('/') || cmd.contains('\\') {
        let p = PathBuf::from(cmd);
        return p.is_file().then_some(p);
    }
    for dir in cli_extra_bin_dirs() {
        let candidate = dir.join(cmd);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(cmd);
            if candidate.is_file() {
                return if candidate.is_absolute() {
                    Some(candidate)
                } else {
                    std::env::current_dir().ok().map(|cwd| cwd.join(candidate))
                };
            }
        }
    }
    None
}

fn which(cmd: &str) -> bool {
    resolve_executable(cmd).is_some()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

fn article_text(html: &str, summary: &str) -> String {
    let from_html = html2text::from_read(html.as_bytes(), 80).unwrap_or_default();
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

pub async fn run_cli(settings: &HashMap<String, String>, prompt: &str) -> Result<AiOutput> {
    AiProvider::from_settings(settings).complete(prompt).await
}

pub async fn summarize_article_data(
    settings: &HashMap<String, String>,
    article: &Article,
) -> Result<AiOutput> {
    let body = truncate(&article_text(&article.content_html, &article.summary), 8000);
    let url = article.url.as_deref().unwrap_or("");
    let prompt = crate::prompts::render(
        settings,
        crate::prompts::PromptKind::Summary,
        &[("title", &article.title), ("url", url), ("body", &body)],
    );
    run_cli(settings, &prompt).await
}

pub async fn summarize_article_table_data(
    settings: &HashMap<String, String>,
    article: &Article,
) -> Result<AiOutput> {
    let body = truncate(&article_text(&article.content_html, &article.summary), 8000);
    let url = article.url.as_deref().unwrap_or("");
    let prompt = crate::prompts::render(
        settings,
        crate::prompts::PromptKind::TableSummary,
        &[("title", &article.title), ("url", url), ("body", &body)],
    );
    run_cli(settings, &prompt).await
}

pub async fn tag_article_data(
    settings: &HashMap<String, String>,
    article: &Article,
) -> Result<AiOutput> {
    let body = truncate(&article_text(&article.content_html, &article.summary), 4000);
    let url = article.url.as_deref().unwrap_or("");
    let prompt = crate::prompts::render(
        settings,
        crate::prompts::PromptKind::Tags,
        &[("title", &article.title), ("url", url), ("body", &body)],
    );
    run_cli(settings, &prompt).await
}

pub async fn folder_brief_data(
    settings: &HashMap<String, String>,
    name: &str,
    items: &[ArticleListItem],
) -> Result<AiOutput> {
    let provider = AiProvider::from_settings(settings);
    if items.is_empty() {
        return Ok(AiOutput {
            command: provider.command,
            provider: provider.id.key().into(),
            output: if crate::prompts::ui_lang(settings) == "tr" {
                "Bu klasörde okunmamış makale yok."
            } else {
                "No unread articles in this folder."
            }
            .into(),
        });
    }
    let mut body = String::new();
    for item in items {
        body.push_str("- ");
        body.push_str(&item.title);
        body.push_str(" (");
        body.push_str(&item.feed_title);
        body.push(')');
        if !item.summary.trim().is_empty() {
            body.push_str("\n  ");
            body.push_str(&item.summary.chars().take(220).collect::<String>());
        }
        body.push('\n');
    }
    let prompt = crate::prompts::render(
        settings,
        crate::prompts::PromptKind::FolderBrief,
        &[("name", name), ("body", &body)],
    );
    provider.complete(&prompt).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn article_text_prefers_longer_of_html_or_summary() {
        let html = "<p>kısa</p>";
        let summary = "bu özet metni html gövdesinden daha uzundur ve seçilmelidir";
        let t = article_text(html, summary);
        assert!(t.contains("seçilmelidir"));
    }

    #[test]
    fn aliases_normalize() {
        assert_eq!(ProviderId::parse("agy"), ProviderId::Antigravity);
        assert_eq!(ProviderId::parse("ANTIGRAVITY"), ProviderId::Antigravity);
        assert_eq!(ProviderId::parse("openai"), ProviderId::ChatGpt);
        assert_eq!(ProviderId::parse("openai-compatible"), ProviderId::ChatGpt);
        assert_eq!(ProviderId::parse("codex"), ProviderId::ChatGpt);
        assert_eq!(ProviderId::parse("kimi"), ProviderId::Kimi);
        assert_eq!(ProviderId::parse("cursor"), ProviderId::Cursor);
    }

    #[test]
    fn antigravity_falls_back_to_agy_when_listed() {
        let mut settings = HashMap::new();
        settings.insert("ai_provider".into(), "agy".into());
        let p = AiProvider::from_settings(&settings);
        assert_eq!(p.id, ProviderId::Antigravity);
        assert!(p.id.binaries().contains(&"agy"));
        assert_eq!(p.id.binaries()[0], "antigravity");
    }

    #[tokio::test]
    async fn stub_cli_returns_demo_text() {
        let stub = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("examples")
            .join("stub-cli")
            .join("feedcairn-ai-stub");
        assert!(stub.exists(), "missing {}", stub.display());
        let mut settings = HashMap::new();
        settings.insert("ai_provider".into(), "chatgpt".into());
        settings.insert("ai_cmd_chatgpt".into(), stub.to_string_lossy().into());
        settings.insert("ai_args_chatgpt".into(), "".into());
        let out = run_cli(&settings, "Title: Hello from tests\nURL: https://example.com\n\nBody")
            .await
            .unwrap();
        assert!(out.output.contains("Feedcairn stub"));
        assert!(out.output.contains("Hello from tests"));
    }

    #[test]
    fn command_for_agy_uses_antigravity_settings() {
        let mut settings = HashMap::new();
        settings.insert("ai_cmd_antigravity".into(), "/tmp/feedcairn-agy-test".into());
        let (cmd, _) = command_for(&settings, "agy");
        assert_eq!(cmd, "/tmp/feedcairn-agy-test");
        assert_eq!(ProviderId::parse("agy").key(), "antigravity");
    }

    #[test]
    fn claude_print_mode_is_text_only_and_resumes_by_session_id() {
        let mut settings = HashMap::new();
        settings.insert("ai_provider".into(), "claude".into());
        settings.insert("ai_cmd_claude".into(), "/opt/feedcairn-test/claude".into());
        settings.insert("ai_model_claude".into(), "sonnet".into());
        let p = AiProvider::from_settings(&settings);
        assert_eq!(p.id, ProviderId::Claude);
        assert_eq!(&p.args[..3], &["-p", "--output-format", "json"]);
        let tools = p.args.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(p.args[tools + 1], "");
        assert!(p.args.iter().any(|a| a == "--strict-mcp-config"));
        assert!(p.args.windows(2).any(|w| w == ["--permission-mode", "dontAsk"]));
        assert!(p.args.windows(2).any(|w| w == ["--model", "sonnet"]));
        assert!(!p.args.iter().any(|a| a == "--effort"));
        assert!(p.session_capable());
        let resumed = build_claude_invocation(Some("abc-123"), &p.args);
        assert_eq!(&resumed[resumed.len() - 2..], &["--resume", "abc-123"]);
    }

    #[test]
    fn claude_json_result_is_unwrapped_and_errors_never_become_text() {
        let ok = r#"{"type":"result","subtype":"success","is_error":false,"result":"Hello","session_id":"s-1"}"#;
        assert_eq!(extract_claude_result(ok).unwrap(), "Hello");
        assert_eq!(extract_claude_session_id(ok).as_deref(), Some("s-1"));
        let bad = r#"{"type":"result","subtype":"error_max_turns","is_error":true}"#;
        assert!(extract_claude_result(bad).is_err());
        assert!(extract_claude_result("not json").is_err());
    }

    #[test]
    fn anthropic_credentials_are_scrubbed() {
        let keys = scrubbed_env_keys();
        for key in ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN", "CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX", "ANTHROPIC_BASE_URL"] {
            assert!(keys.iter().any(|k| k == key), "{key}");
        }
    }

    #[test]
    fn cursor_runs_headless_with_workspace_trust_like_tabulect() {
        for (cmd, agent) in [("/opt/feedcairn-test/cursor-agent", false), ("/opt/feedcairn-test/cursor", true)] {
            let mut settings = HashMap::new();
            settings.insert("ai_provider".into(), "cursor".into());
            settings.insert("ai_cmd_cursor".into(), cmd.into());
            let p = AiProvider::from_settings(&settings);
            assert_eq!(p.args.first().map(String::as_str) == Some("agent"), agent, "{cmd}");
            assert!(p.args.iter().any(|a| a == "--trust"));
            assert!(p.args.windows(2).any(|w| w == ["--mode", "ask"]));
            assert!(p.args.windows(2).any(|w| w[0] == "--workspace" && w[1].ends_with("ai-session")));
            assert!(!p.args.iter().any(|a| a == "--force" || a == "-f"));
            assert_eq!(p.args.last().map(String::as_str), Some("{prompt}"));
        }
    }

    #[test]
    fn ui_never_sees_full_cli_paths() {
        assert_eq!(crate::models::display_command("/opt/tools/bin/kimi"), "kimi");
        assert_eq!(crate::models::display_command("claude"), "claude");
        let out = AiOutput { provider: "kimi".into(), command: "/home/x/.local/bin/kimi".into(), output: String::new() };
        assert_eq!(serde_json::to_value(&out).unwrap()["command"], "kimi");
        if let Some(home) = dirs::home_dir() {
            let text = format!("{}/.local/bin/kimi failed", home.display());
            assert_eq!(crate::models::mask_home(&text), "~/.local/bin/kimi failed");
        }
        let hint = install_hint("kimi", "/home/x/.local/bin/kimi");
        assert!(!hint.contains("/home/x"), "{hint}");
    }

    #[test]
    fn missing_cli_probe() {
        let mut settings = HashMap::new();
        settings.insert("ai_provider".into(), "chatgpt".into());
        settings.insert(
            "ai_cmd_chatgpt".into(),
            "definitely-not-a-real-feedcairn-cli".into(),
        );
        let p = probe(&settings);
        assert!(!p.found);
        assert!(p.hint.contains("not found"));
    }

    #[test]
    fn codex_args_use_exec_model_and_effort_no_ephemeral() {
        let mut settings = HashMap::new();
        settings.insert("ai_provider".into(), "chatgpt".into());
        settings.insert("ai_cmd_chatgpt".into(), "codex".into());
        settings.insert("ai_args_chatgpt".into(), "".into());
        settings.insert("ai_model_chatgpt".into(), "gpt-6-astra".into());
        settings.insert("ai_effort_chatgpt".into(), "xhigh".into());
        let p = AiProvider::from_settings(&settings);
        assert!(
            is_codex_command(&p.command),
            "expected codex binary, got {}",
            p.command
        );
        assert!(p.args.starts_with(&["exec".into(), "--skip-git-repo-check".into()][..]));
        assert!(p.args.contains(&"-C".into()));
        assert!(p.args.contains(&"-m".into()));
        assert!(p.args.contains(&"gpt-6-astra".into()));
        assert!(p.args.iter().any(|a| a.contains("model_reasoning_effort=\"xhigh\"")));
        assert!(p.args.contains(&"--json".into()));
        assert_eq!(p.args.last().map(String::as_str), Some("-"));
        assert!(!p.args.iter().any(|a| a == "--quiet"));
        assert!(
            !p.args.iter().any(|a| a == "--ephemeral"),
            "session reuse requires persisted Codex sessions"
        );
    }

    #[test]
    fn codex_resume_args_shape() {
        let work = PathBuf::from("/tmp/feedcairn-ai-session-test");
        let out = work.join("last.txt");
        let base = vec![
            "exec".into(),
            "--skip-git-repo-check".into(),
            "-C".into(),
            work.to_string_lossy().to_string(),
            "-s".into(),
            "read-only".into(),
            "-m".into(),
            "gpt-6-astra".into(),
            "-c".into(),
            "model_reasoning_effort=\"xhigh\"".into(),
            "--json".into(),
            "-".into(),
        ];
        let first = build_codex_invocation(None, &base, &work, &out);
        assert_eq!(first[0], "exec");
        assert!(!first.iter().any(|a| a == "resume"));
        assert!(!first.iter().any(|a| a == "--ephemeral"));
        assert!(first.contains(&"-C".into()));

        let resume = build_codex_invocation(Some("019abcde-1111-2222-3333-444455556666"), &base, &work, &out);
        assert_eq!(
            &resume[..4],
            &[
                "exec".to_string(),
                "resume".to_string(),
                "019abcde-1111-2222-3333-444455556666".to_string(),
                "--skip-git-repo-check".to_string(),
            ][..]
        );
        assert!(!resume.iter().any(|a| a == "--ephemeral"));
        assert_eq!(resume.last().map(String::as_str), Some("-"));
        assert!(resume.contains(&"--json".into()));
        assert!(resume.contains(&"-o".into()));
    }

    #[test]
    fn extracts_thread_started_id() {
        let jsonl = r#"{"type":"thread.started","thread_id":"019f0000-aaaa-bbbb-cccc-ddddeeeeffff"}
{"type":"agent_message","message":"hello"}
"#;
        assert_eq!(
            extract_codex_thread_id(jsonl).as_deref(),
            Some("019f0000-aaaa-bbbb-cccc-ddddeeeeffff")
        );
    }

    #[test]
    fn openai_api_cli_is_never_a_default_binary() {
        assert!(!ProviderId::ChatGpt.binaries().contains(&"openai"));
        assert!(scrubbed_env_keys().iter().any(|k| k == "OPENAI_API_KEY"));
    }

    #[test]
    fn legacy_quiet_only_args_are_ignored() {
        let mut settings = HashMap::new();
        settings.insert("ai_provider".into(), "chatgpt".into());
        settings.insert("ai_cmd_chatgpt".into(), "codex".into());
        settings.insert("ai_args_chatgpt".into(), "--quiet".into());
        let p = AiProvider::from_settings(&settings);
        assert!(!p.args.iter().any(|a| a == "--quiet"));
        assert!(p.args.contains(&"exec".into()));
        assert!(!p.args.iter().any(|a| a == "--ephemeral"));
    }

    #[test]
    fn kimi_args_quiet_model_thinking() {
        let mut settings = HashMap::new();
        settings.insert("ai_provider".into(), "kimi".into());
        settings.insert("ai_cmd_kimi".into(), "kimi".into());
        settings.insert("ai_args_kimi".into(), "".into());
        settings.insert("ai_model_kimi".into(), "kimi-code/k3".into());
        settings.insert("ai_thinking_kimi".into(), "true".into());
        let p = AiProvider::from_settings(&settings);
        assert_eq!(p.args[0], "--quiet");
        assert!(p.args.contains(&"--model".into()));
        assert!(p.args.contains(&"kimi-code/k3".into()));
        assert!(p.args.contains(&"--thinking".into()));
    }

    #[test]
    fn cursor_args_agent_prompt_model_effort() {
        let mut settings = HashMap::new();
        settings.insert("ai_provider".into(), "cursor".into());
        settings.insert("ai_cmd_cursor".into(), "cursor".into());
        settings.insert("ai_args_cursor".into(), "".into());
        settings.insert("ai_model_cursor".into(), "composer-2.5[effort=high]".into());
        settings.insert("ai_effort_cursor".into(), "high".into());
        let p = AiProvider::from_settings(&settings);
        assert_eq!(p.args[0], "agent");
        assert!(p.args.contains(&"-p".into()));
        assert!(p.args.windows(2).any(|w| w[0] == "--model" && w[1] == "composer-2.5"));
        assert!(!p.args.iter().any(|a| a.contains("[effort")));
        assert!(p.args.iter().any(|a| a == "{prompt}"));
    }

    #[test]
    fn cursor_model_defaults_to_auto() {
        assert_eq!(cursor_model_id(""), "auto");
        assert_eq!(cursor_model_id("composer-1"), "auto");
        assert_eq!(cursor_model_id("composer-1[effort=high]"), "auto");
        assert_eq!(cursor_model_id("default"), "auto");
        assert_eq!(cursor_model_id("gpt-5.3-codex-high"), "gpt-5.3-codex-high");
        assert_eq!(cursor_model_id("bad model; rm"), "auto");
        let mut settings = HashMap::new();
        settings.insert("ai_provider".into(), "cursor".into());
        settings.insert("ai_cmd_cursor".into(), "cursor-agent".into());
        settings.insert("ai_args_cursor".into(), "".into());
        let p = AiProvider::from_settings(&settings);
        assert!(p.args.windows(2).any(|w| w[0] == "--model" && w[1] == "auto"));
    }
}
