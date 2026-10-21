//! "Subscriptions / Abonelikler": each user connects their OWN AI CLI account.
//!
//! Feedcairn never stores, copies or reads tokens. It only
//! 1. checks whether each official CLI is installed,
//! 2. opens that CLI's official login command in a terminal window, and
//! 3. asks the CLI itself whether the current user is signed in.

use crate::error::{Error, Result};
use serde::Serialize;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Clone, Copy)]
struct Definition {
    id: &'static str,
    name: &'static str,
    binaries: &'static [&'static str],
    login: &'static [&'static str],
    status: &'static [&'static str],
    install: &'static str,
    docs: &'static str,
}

const DEFINITIONS: &[Definition] = &[
    Definition {
        id: "chatgpt",
        name: "ChatGPT (Codex CLI)",
        binaries: &["codex"],
        login: &["login"],
        status: &["login", "status"],
        install: "npm install -g @openai/codex",
        docs: "https://developers.openai.com/codex/auth/",
    },
    Definition {
        id: "kimi",
        name: "Kimi (Kimi Code CLI)",
        binaries: &["kimi"],
        login: &["login"],
        status: &["provider", "list", "--json"],
        install: "npm install -g @moonshot-ai/kimi-code",
        docs: "https://moonshotai.github.io/kimi-code/en/guides/getting-started",
    },
    Definition {
        id: "gemini",
        name: "Google Gemini (Antigravity CLI)",
        binaries: &["agy", "antigravity"],
        login: &[],
        status: &["-p", "/usage", "--output-format", "json"],
        install: "curl -fsSL https://antigravity.google/cli/install.sh | bash",
        docs: "https://antigravity.google/docs/cli/install/",
    },
    Definition {
        id: "cursor",
        name: "Cursor (Cursor Agent CLI)",
        binaries: &["cursor-agent"],
        login: &["login"],
        status: &["status"],
        install: "curl https://cursor.com/install -fsS | bash",
        docs: "https://cursor.com/docs/cli/overview",
    },
    Definition {
        id: "claude",
        name: "Claude (Claude Code CLI)",
        binaries: &["claude"],
        login: &["auth", "login"],
        status: &["auth", "status"],
        install: "curl -fsSL https://claude.ai/install.sh | bash",
        docs: "https://code.claude.com/docs/en/authentication",
    },
];

#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionStatus {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub command: Option<String>,
    /// "connected" | "login_required" | "api_key" | "unknown" | "missing"
    pub status: String,
    pub install: String,
    pub login_command: String,
    pub docs: String,
}

fn find_binary(def: &Definition) -> Option<String> {
    def.binaries
        .iter()
        .find_map(|b| crate::ai::resolve_executable(b))
        .map(|p| p.to_string_lossy().into_owned())
}

fn login_command_text(def: &Definition, bin: &str) -> String {
    let name = std::path::Path::new(bin)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(bin);
    if def.login.is_empty() {
        name.to_string()
    } else {
        format!("{name} {}", def.login.join(" "))
    }
}

/// The JSON object in a CLI's output, tolerating a banner or update notice before it.
fn parse_json_object(stdout: &str) -> Option<serde_json::Value> {
    let trimmed = stdout.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return v.is_object().then_some(v);
    }
    let (start, end) = (trimmed.find('{')?, trimmed.rfind('}')?);
    if end <= start {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(&trimmed[start..=end]).ok().filter(|v| v.is_object())
}

/// Kimi Code keeps its OAuth login in one of these files under the home folder. Like Tabulect,
/// Feedcairn only checks that one exists (never reads it) when the status output was unreadable.
const KIMI_LOGIN_FILES: &[&str] = &[
    ".kimi/credentials/kimi-code.json",
    ".kimi/credentials.json",
    ".kimi-code/credentials/kimi-code.json",
    ".config/kimi/auth.json",
];

fn kimi_login_file_exists() -> bool {
    dirs::home_dir().is_some_and(|home| KIMI_LOGIN_FILES.iter().any(|rel| home.join(rel).is_file()))
}

fn classify(def: &Definition, ok: bool, stdout: &str, stderr: &str) -> &'static str {
    let all = format!("{stdout}\n{stderr}").to_lowercase();
    if all.contains("not logged") || all.contains("login required") || all.contains("unauthenticated") {
        return "login_required";
    }
    match def.id {
        "chatgpt" => {
            if all.contains("using chatgpt") {
                "connected"
            } else if all.contains("api key") {
                "api_key"
            } else {
                "login_required"
            }
        }
        "kimi" => match parse_json_object(stdout) {
            Some(v) => {
                let oauth = v
                    .get("providers")
                    .and_then(|p| p.as_object())
                    .map(|map| {
                        map.values().any(|p| {
                            p.get("oauth").map(|o| !o.is_null() && o != false).unwrap_or(false)
                                && p.get("type").and_then(|t| t.as_str()) == Some("kimi")
                        })
                    })
                    .unwrap_or(false);
                if oauth { "connected" } else { "login_required" }
            }
            None => "unknown",
        },
        // `claude auth status` prints JSON and exits 1 when signed out. Only a claude.ai
        // subscription login counts; API keys, apiKeyHelper, long-lived OAuth tokens and
        // Bedrock/Vertex/Foundry are metered or org credentials, not the user's subscription.
        "claude" => match serde_json::from_str::<serde_json::Value>(stdout.trim()) {
            Ok(v) => {
                let logged_in = v.get("loggedIn").and_then(|b| b.as_bool()).unwrap_or(false);
                match (logged_in, v.get("authMethod").and_then(|m| m.as_str()).unwrap_or("none")) {
                    (true, "claude.ai") => "connected",
                    (true, _) => "api_key",
                    _ => "login_required",
                }
            }
            Err(_) if !ok => "login_required",
            Err(_) => "unknown",
        },
        "gemini" => {
            if ok && stdout.contains('{') { "connected" } else { "unknown" }
        }
        _ => {
            if all.contains("logged in") || all.contains("signed in") { "connected" } else { "unknown" }
        }
    }
}

async fn probe(def: &Definition) -> SubscriptionStatus {
    let bin = find_binary(def);
    let mut status = SubscriptionStatus {
        id: def.id.into(),
        name: def.name.into(),
        installed: bin.is_some(),
        // File name only: the full path usually contains the home folder.
        command: bin.as_deref().map(crate::models::display_command),
        status: "missing".into(),
        install: def.install.into(),
        login_command: String::new(),
        docs: def.docs.into(),
    };
    let Some(bin) = bin else { return status };
    status.login_command = login_command_text(def, &bin);
    let mut cmd = Command::new(&bin);
    cmd.args(def.status)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .env("PATH", crate::ai::augmented_path_env())
        .env("NO_COLOR", "1");
    for key in crate::ai::scrubbed_env_keys() {
        cmd.env_remove(key);
    }
    // Kimi can take a while to start (update check); give it the same room as Polyquill.
    let wait = if def.id == "kimi" { 20 } else { 12 };
    status.status = match tokio::time::timeout(Duration::from_secs(wait), cmd.output()).await {
        Ok(Ok(out)) => classify(
            def,
            out.status.success(),
            &String::from_utf8_lossy(&out.stdout),
            &String::from_utf8_lossy(&out.stderr),
        )
        .into(),
        _ => "unknown".into(),
    };
    if def.id == "kimi" && status.status == "unknown" && kimi_login_file_exists() {
        status.status = "connected".into();
    }
    status
}

pub async fn statuses() -> Vec<SubscriptionStatus> {
    futures::future::join_all(DEFINITIONS.iter().map(probe)).await
}

/// Open the provider's OFFICIAL login command in a terminal window. Only fixed,
/// built-in commands are ever run; nothing from the request is executed.
pub fn open_login(id: &str) -> Result<String> {
    let def = DEFINITIONS
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| Error::msg(format!("unknown subscription `{id}`")))?;
    let bin = find_binary(def).ok_or_else(|| {
        Error::msg(format!("{} is not installed. Install it first: {}", def.name, def.install))
    })?;
    let unset = crate::ai::scrubbed_env_keys().join(" ");
    let quoted = shell_quote(&bin);
    let line = if def.login.is_empty() {
        quoted
    } else {
        format!("{quoted} {}", def.login.join(" "))
    };
    let script = format!(
        "#!/bin/bash\nunset {unset}\necho \"Feedcairn: {name}\"\n{line}\nread -r -p \"Return to Feedcairn and press Re-check. / Feedcairn'e dönüp Yeniden denetle'ye basın. [Enter] \"\n",
        name = def.name
    );
    let dir = std::env::temp_dir().join(format!("feedcairn-login-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("login-{}.command", def.id));
    std::fs::write(&path, script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    }
    let path_str = path.to_string_lossy().to_string();
    let launched = if cfg!(target_os = "macos") {
        std::process::Command::new("open").args(["-a", "Terminal", &path_str]).spawn().is_ok()
    } else if cfg!(target_os = "linux") {
        ["x-terminal-emulator", "gnome-terminal", "konsole", "xterm"].iter().any(|term| {
            let args: Vec<&str> = if *term == "gnome-terminal" {
                vec!["--", "bash", &path_str]
            } else {
                vec!["-e", "bash", &path_str]
            };
            std::process::Command::new(term).args(&args).spawn().is_ok()
        })
    } else {
        false
    };
    let human = login_command_text(def, &bin);
    if launched {
        Ok(format!("Opened `{human}` in a terminal. Finish signing in, then press Re-check."))
    } else {
        Err(Error::msg(format!("Could not open a terminal. Run `{human}` yourself, then press Re-check.")))
    }
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_api_key_login_is_not_a_subscription() {
        let def = &DEFINITIONS[0];
        assert_eq!(classify(def, true, "Logged in using ChatGPT", ""), "connected");
        assert_eq!(classify(def, true, "Logged in using an API key - sk-***", ""), "api_key");
        assert_eq!(classify(def, false, "", "Not logged in"), "login_required");
    }

    #[test]
    fn kimi_requires_oauth_provider() {
        let def = &DEFINITIONS[1];
        let oauth = r#"{"providers":{"k":{"type":"kimi","oauth":{"x":1}}},"models":{}}"#;
        let key = r#"{"providers":{"k":{"type":"kimi","api_key":"***"}},"models":{}}"#;
        assert_eq!(classify(def, true, oauth, ""), "connected");
        assert_eq!(classify(def, true, key, ""), "login_required");
    }

    #[test]
    fn claude_requires_a_claude_ai_subscription_login() {
        let def = DEFINITIONS.iter().find(|d| d.id == "claude").unwrap();
        assert_eq!(def.login, &["auth", "login"]);
        assert_eq!(def.status, &["auth", "status"]);
        let sub = r#"{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty"}"#;
        assert_eq!(classify(def, true, sub, ""), "connected");
        for method in ["api_key", "api_key_helper", "oauth_token", "third_party"] {
            let other = format!(r#"{{"loggedIn":true,"authMethod":"{method}"}}"#);
            assert_eq!(classify(def, true, &other, ""), "api_key", "{method}");
        }
        assert_eq!(classify(def, false, r#"{"loggedIn":false,"authMethod":"none"}"#, ""), "login_required");
    }

    #[test]
    fn kimi_status_tolerates_a_banner_and_matches_tabulect() {
        let def = DEFINITIONS.iter().find(|d| d.id == "kimi").unwrap();
        let oauth = r#"{"providers":{"k":{"type":"kimi","oauth":{"x":1}}},"models":{}}"#;
        assert_eq!(classify(def, true, &format!("Kimi Code v1.4 checking for updates\n{oauth}\n"), ""), "connected");
        assert_eq!(classify(def, true, "", ""), "unknown");
        assert_eq!(classify(def, false, "", "Error: not logged in"), "login_required");
    }

    #[test]
    fn shell_quote_escapes() {
        assert_eq!(shell_quote("/a b/it's"), "'/a b/it'\\''s'");
    }
}
