#!/usr/bin/env node
/**
 * Release guard: fails (exit 1) if a build output or source tree contains any AI CLI
 * login state, credential/token file, bundled AI CLI, or private local data.
 *
 * Usage: node scripts/check-no-credentials.mjs [--source] <dir-or-.app> [more paths...]
 *   --source  scan a source checkout: skips .git/, node_modules/, dist/, target/
 *
 * Each user must connect their OWN CLI account (codex login, kimi login, agy,
 * cursor-agent login). Nothing that could carry somebody else's login may ship.
 */
import { lstat, readdir, readFile } from "node:fs/promises";
import { basename, join, relative } from "node:path";

const FORBIDDEN_DIRS = new Set([
  ".codex", ".chatgpt", ".kimi", ".kimi-code", ".gemini", ".antigravity", ".cursor",
  ".claude", ".config", "Keychains", ".git", ".local", "agent-tools", "uploads",
]);
const FORBIDDEN_PACKAGES = ["@moonshot-ai/kimi-code", "@openai/codex", "@google/gemini-cli", "@anthropic-ai/claude-code"];
const FORBIDDEN_FILES = [
  /^auth\.json$/i,
  /^credentials?(\..*)?$/i,
  /^\.credentials\.json$/i, // Claude Code login (Linux/Windows)
  /^\.claude\.json$/i,
  /oauth/i,
  /^cli-config\.json$/i,
  /^models_cache\.json$/i,
  /^\.env(\..*)?$/i,
  /\.(pem|p12|pfx|key|keychain-db|keychain)$/i,
  /^(cookies|login data)$/i,
  /^id_(rsa|ed25519|ecdsa)$/i,
  /\.(opml)$/i,
  /\.(db|sqlite|sqlite3)$/i,
  /^\._/,
  /^\.DS_Store$/,
];
const ALLOWED_FILES = [/^sample\.opml$/i];
const CONTENT_PATTERNS = [
  [/-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/, "private key"],
  [/\b(?:sk-[A-Za-z0-9_-]{20,}|AIza[0-9A-Za-z_-]{35}|ghp_[A-Za-z0-9]{36}|xox[baprs]-[A-Za-z0-9-]{10,})\b/, "API token"],
  [/"(?:access_token|refresh_token|id_token)"\s*:\s*"[^"]{12,}"/, "OAuth token"],
  [/\/Users\/[A-Za-z0-9._-]+\//, "absolute user home path"],
];
const TEXT_EXT = /\.(m?js|cjs|ts|tsx|json|md|html|css|txt|sh|command|toml|ya?ml|plist|swift|rs|xml|svg)$/i;

const problems = [];
const SOURCE_SKIP = new Set([".git", "node_modules", "dist", "target"]);
let sourceMode = false;

async function walk(path, root) {
  const info = await lstat(path);
  const name = basename(path);
  const rel = relative(root, path) || name;
  if (info.isSymbolicLink()) {
    problems.push(`${rel}: symbolic link (could point at a login file)`);
    return;
  }
  if (info.isDirectory()) {
    if (sourceMode && SOURCE_SKIP.has(name)) return;
    if (FORBIDDEN_DIRS.has(name) && path !== root) {
      problems.push(`${rel}/: login/state or private directory`);
      return;
    }
    if (name === "node_modules") {
      for (const pkg of FORBIDDEN_PACKAGES) {
        try {
          await lstat(join(path, pkg));
          problems.push(`${rel}/${pkg}: bundled AI CLI (users must install their own)`);
        } catch { /* absent */ }
      }
      return; // third-party code is out of scope for the content scan
    }
    for (const entry of await readdir(path)) await walk(join(path, entry), root);
    return;
  }
  if (!ALLOWED_FILES.some((re) => re.test(name)) && FORBIDDEN_FILES.some((re) => re.test(name))) {
    problems.push(`${rel}: forbidden file name`);
    return;
  }
  if (TEXT_EXT.test(name) && info.size < 2 * 1024 * 1024 && !/check-no-credentials\.mjs$/.test(name)) {
    const text = await readFile(path, "utf8");
    for (const [re, label] of CONTENT_PATTERNS) {
      if (re.test(text)) problems.push(`${rel}: contains ${label}`);
    }
  }
}

const args = process.argv.slice(2);
sourceMode = args.includes("--source");
const targets = args.filter((arg) => arg !== "--source");
if (!targets.length) {
  console.error("usage: check-no-credentials.mjs <path> [...]");
  process.exit(2);
}
for (const target of targets) await walk(target, target);
if (problems.length) {
  console.error("Credential/private-data guard FAILED:\n  " + problems.join("\n  "));
  process.exit(1);
}
console.log(`Credential/private-data guard passed for: ${targets.join(", ")}`);
