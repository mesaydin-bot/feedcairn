# Feedcairn

Feedcairn is a local-first desktop RSS/Atom reader: feed tree with folders and unread counts, article list, sanitized reader, star / read later, full-text search, OPML import/export, notes, and optional AI summaries, tables, tags and folder briefings using the AI subscription you already have.

- Desktop app: Tauri 2 + React/TypeScript + Rust (axum) + SQLite
- Bundle id: `org.thinkingtoolslab.feedcairn`
- Ships **empty**: no feeds are preloaded. Import your own OPML or add feeds by URL.
- License: MIT (see `LICENSE`)

## Install

Requirements: Node.js 20+, Rust stable (`rustup`), and the Tauri 2 prerequisites (macOS: `xcode-select --install`; Debian/Ubuntu: `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf`).

```bash
npm install
npm run tauri:dev        # desktop app
npm run tauri:build      # release bundle + credential check
```

The SQLite database lives in the OS app-data folder (override with `FEEDCAIRN_DB=/path/to/feedcairn.db`).

Browser preview (same backend, no WebView):

```bash
export FEEDCAIRN_DB="$PWD/data/feedcairn.db"
npm run web              # UI http://127.0.0.1:43123 · API http://127.0.0.1:43124
```

## Import your feeds

Export OPML from your current reader, then in Feedcairn use **Import / export OPML**. Nested outlines become folders; duplicate feed URLs are skipped. A small generic example is available as `sample.opml` (download link in the import dialog; it is never loaded automatically).

## Bring your own AI subscription

Feedcairn does not include, bundle or log in to any AI service. Every user connects **their own** subscription through the provider's official command-line tool (CLI). Feedcairn only runs the CLI you installed, under your own operating-system account.

1. Install the CLI(s) you want to use:

| Provider | Install (official) | Sign in | Status check Feedcairn runs |
| --- | --- | --- | --- |
| ChatGPT (Codex CLI) | `npm install -g @openai/codex` | `codex login` (choose *Sign in with ChatGPT*) | `codex login status` — API-key logins are rejected |
| Kimi (Kimi Code CLI) | `npm install -g @moonshot-ai/kimi-code` | `kimi login` | `kimi provider list --json` |
| Google Gemini (Antigravity CLI `agy`) | see https://antigravity.google/docs/cli/install/ | run `agy` once and sign in with Google | `agy -p /usage --output-format json` |
| Cursor (Cursor Agent CLI) | `curl https://cursor.com/install -fsS \| bash` | `cursor-agent login` | `cursor-agent status` |
| Claude (Claude Code CLI `claude`) | `curl -fsSL https://claude.ai/install.sh \| bash` (or `brew install --cask claude-code`) | `claude auth login` (sign in with your Claude Pro/Max/Team/Enterprise account) | `claude auth status` — only a claude.ai subscription login counts; API keys, `ANTHROPIC_*` / `CLAUDE_CODE_OAUTH_TOKEN` variables, `apiKeyHelper` and Bedrock/Vertex/Foundry are refused |

2. Open **Subscriptions / Abonelikler** in Feedcairn. It detects each CLI, shows whether you are signed in, and its **Sign in** button opens the official login command in a terminal window. Press **Re-check** when done.

Rules Feedcairn follows:

- It never reads, copies, stores or logs passwords, OAuth tokens or API keys. Logins stay in each CLI's own store in your home directory or keychain.
- It removes API-key and base-URL environment variables (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, `CURSOR_API_KEY`, `*_BASE_URL`, …) before starting a CLI, so calls always use your subscription login, not a key that happens to be in your shell.
- Codex runs with a read-only sandbox and `forced_login_method="chatgpt"`; Cursor runs as `cursor-agent -p --output-format text --trust --workspace <Feedcairn AI folder> --mode ask` (the workspace trust flag only trusts that dedicated folder; no force flags).
- Claude Code runs in print mode with every tool disabled (`claude -p --output-format json --tools "" --strict-mcp-config --disable-slash-commands --permission-mode dontAsk`, optional `--model` / `--effort` from Settings); follow-up asks in the same session use `--resume <session id>`. Claude Code keeps its own transcripts under `~/.claude`.
- Release builds run `scripts/check-no-credentials.mjs`, which fails the build if any CLI login folder, token/credential file, private key or bundled AI CLI ends up in the package.

Install commands and URLs belong to the providers and can change; check their docs if a command fails.

**Settings → AI provider** chooses the CLI. The advanced command template (custom executable/arguments) can only be changed in the desktop app, never over the local web API. A demo stub CLI for testing without any account lives in `examples/stub-cli/`.

## Language, prompts and your own prompts

- The interface is bilingual (English / Türkçe). The language switch is persisted on your computer; the default follows your OS language.
- All built-in AI prompts are **English by default**. When the interface is Turkish, the Turkish translations of the built-in prompts are used.
- Open **Settings → AI prompts** to edit the built-in prompts (summary, table summary, tags, folder briefing) or write your own; **Reset to default** restores the built-in text. Custom prompts are stored in the local SQLite settings. Placeholders: `{title}`, `{url}`, `{body}`, `{name}`. Saved AI command templates are available from the search bar.

Free-form AI answers (type `/…` in the search bar) follow the language of your question; if that is unclear, the interface language.

## Keyboard

| Key | Action |
| --- | --- |
| `j` / `n` / `↓` | Next article |
| `k` / `p` / `↑` | Previous article |
| `m` | Toggle read |
| `s` | Star / read later |
| `o` / `v` | Open original |
| `r` | Refresh current view |
| `/` | Search (start with `/` for an AI instruction) |
| `?` | Cheat sheet |

## Security

The optional web API binds to loopback only (non-loopback addresses are refused), checks `Host` and `Origin`, rejects cross-site requests, requires a random per-launch token (`X-Feedcairn-Token`), accepts JSON only, and has no permissive CORS. See `PRIVACY.md`.

## Tests

```bash
npm test                 # cargo unit tests
npm run typecheck
npm run check:credentials
```

## Türkçe

Feedcairn yerel çalışan bir RSS/Atom okuyucusudur: klasörlü kaynak ağacı, okunmamış sayaçları, okuma paneli, yıldız / sonra oku, arama, OPML içe/dışa aktarma, notlar ve kendi AI aboneliğinizle isteğe bağlı özet, tablo, etiket ve klasör brifingi.

- Kurulum: `npm install`, `npm run tauri:dev`.
- **Boş gelir**: hazır kaynak yoktur; kendi OPML dosyanızı içe aktarın veya URL ile kaynak ekleyin.
- AI: Feedcairn hiçbir hesap içermez. Kendi CLI'nizi kurun, **Ayarlar → Abonelikler** bölümünden resmî giriş komutunu açın ve **Yeniden denetle**'ye basın. Token veya API anahtarı saklanmaz.
- Arayüz dili **Ayarlar**'dan değiştirilir. Yerleşik AI istemleri varsayılan olarak İngilizcedir; arayüz Türkçeyken Türkçe çevirileri kullanılır. **Ayarlar → AI istemleri** bölümünde her istemi düzenleyebilir ve **Varsayılana dön** ile sıfırlayabilirsiniz. Serbest AI yanıtları sorunuzun dilinde verilir.
- Gizlilik: bkz. `PRIVACY.md`.
