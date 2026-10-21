import { invoke } from "@tauri-apps/api/core";
import type {
  AiOutput,
  AiProbe,
  Article,
  ArticleFilter,
  ArticleListItem,
  AskAiRequest,
  AskAiResponse,
  Bootstrap,
  Feed,
  Folder,
  ImportResult,
  Note,
  PromptDefault,
  PromptTemplate,
  RefreshAllResult,
  RefreshResult,
  Setting,
  SubscriptionStatus,
} from "./types";
import type { ApiError } from "./types";

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function parseError(res: Response): Promise<never> {
  let body: ApiError = { error: res.statusText };
  try {
    body = await res.json();
  } catch {
    /* ignore */
  }
  throw body;
}

let sessionToken: Promise<string> | null = null;

/** Per-launch token of the optional web/API mode (`npm run web`). Desktop mode uses Tauri IPC. */
function getSessionToken(refresh = false): Promise<string> {
  if (!sessionToken || refresh) {
    sessionToken = fetch("/api/session", { cache: "no-store" })
      .then((res) => (res.ok ? res.json() : Promise.reject(new Error(`session ${res.status}`))))
      .then((body: { token?: string }) => String(body.token || ""))
      .catch((error) => {
        sessionToken = null;
        throw error;
      });
  }
  return sessionToken;
}

async function authedFetch(url: string, init?: RequestInit, contentType = true): Promise<Response> {
  const send = async (refresh: boolean) =>
    fetch(url, {
      ...init,
      headers: {
        ...(contentType ? { "Content-Type": "application/json" } : {}),
        ...(init?.headers || {}),
        "X-Feedcairn-Token": await getSessionToken(refresh),
      },
    });
  const res = await send(false);
  return res.status === 401 ? send(true) : res;
}

async function json<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await authedFetch(url, init);
  if (!res.ok) await parseError(res);
  if (res.status === 204) return undefined as T;
  const ct = res.headers.get("content-type") || "";
  if (ct.includes("xml") || ct.includes("text/plain")) {
    return (await res.text()) as T;
  }
  return res.json();
}

function tauriErr(e: unknown): never {
  // IMPORTANT: do not `throw` parsed JSON inside try/catch — that rethrows the raw string.
  if (typeof e === "string") {
    let parsed: ApiError | null = null;
    try {
      const v = JSON.parse(e) as ApiError;
      if (
        v &&
        typeof v === "object" &&
        (typeof v.error === "string" || typeof v.hint === "string")
      ) {
        parsed = v;
      }
    } catch {
      /* not JSON */
    }
    if (parsed) throw parsed;
    throw { error: e } satisfies ApiError;
  }
  if (e && typeof e === "object" && ("error" in e || "hint" in e)) {
    throw e as ApiError;
  }
  throw { error: e instanceof Error ? e.message : String(e) } satisfies ApiError;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    tauriErr(e);
  }
}

function qs(filter: ArticleFilter): string {
  const p = new URLSearchParams();
  if (filter.feed_id != null) p.set("feed_id", String(filter.feed_id));
  if (filter.folder_id != null) p.set("folder_id", String(filter.folder_id));
  if (filter.saved_only) p.set("saved_only", "true");
  if (filter.unread_only) p.set("unread_only", "true");
  if (filter.query) p.set("query", filter.query);
  if (filter.limit) p.set("limit", String(filter.limit));
  if (filter.offset) p.set("offset", String(filter.offset));
  return p.toString();
}

export const api = {
  bootstrap(): Promise<Bootstrap> {
    return isTauri() ? call("bootstrap") : json("/api/bootstrap");
  },
  createFolder(name: string, parent_id?: number | null): Promise<Folder> {
    const req = { name, parent_id: parent_id ?? null };
    return isTauri()
      ? call("create_folder", { req })
      : json("/api/folders", { method: "POST", body: JSON.stringify(req) });
  },
  renameFolder(id: number, name: string): Promise<void> {
    return isTauri()
      ? call("rename_folder", { id, name })
      : json(`/api/folders/${id}/rename`, {
          method: "POST",
          body: JSON.stringify({ name }),
        });
  },
  deleteFolder(id: number): Promise<void> {
    return isTauri()
      ? call("delete_folder", { id })
      : json(`/api/folders/${id}`, { method: "DELETE" });
  },
  addFeed(url: string, folder_id?: number | null, title?: string): Promise<Feed> {
    const req = { url, folder_id: folder_id ?? null, title: title || null };
    return isTauri()
      ? call("add_feed", { req })
      : json("/api/feeds", { method: "POST", body: JSON.stringify(req) });
  },
  updateFeed(
    id: number,
    patch: { title?: string; folder_id?: number; unfile?: boolean },
  ): Promise<Feed> {
    return isTauri()
      ? call("update_feed", { id, req: patch })
      : json(`/api/feeds/${id}`, { method: "PATCH", body: JSON.stringify(patch) });
  },
  deleteFeed(id: number): Promise<void> {
    return isTauri()
      ? call("delete_feed", { id })
      : json(`/api/feeds/${id}`, { method: "DELETE" });
  },
  listArticles(filter: ArticleFilter): Promise<ArticleListItem[]> {
    return isTauri()
      ? call("list_articles", { filter })
      : json(`/api/articles?${qs(filter)}`);
  },
  getArticle(id: number): Promise<Article> {
    return isTauri() ? call("get_article", { id }) : json(`/api/articles/${id}`);
  },
  setRead(id: number, value: boolean): Promise<Article> {
    return isTauri()
      ? call("set_read", { id, isRead: value })
      : json(`/api/articles/${id}/read`, {
          method: "POST",
          body: JSON.stringify({ value }),
        });
  },
  setSaved(id: number, value: boolean): Promise<Article> {
    return isTauri()
      ? call("set_saved", { id, isSaved: value })
      : json(`/api/articles/${id}/save`, {
          method: "POST",
          body: JSON.stringify({ value }),
        });
  },
  markAllRead(filter: ArticleFilter): Promise<{ updated: number } | number> {
    return isTauri()
      ? call("mark_all_read", { filter })
      : json("/api/articles/mark-all-read", {
          method: "POST",
          body: JSON.stringify(filter),
        });
  },
  refreshFeed(id: number): Promise<RefreshResult> {
    return isTauri()
      ? call("refresh_feed", { id })
      : json(`/api/refresh/${id}`, { method: "POST" });
  },
  refreshAll(folder_id?: number | null): Promise<RefreshAllResult> {
    return isTauri()
      ? call("refresh_all", { folderId: folder_id ?? null })
      : json(
          `/api/refresh${folder_id ? `?folder_id=${folder_id}` : ""}`,
          { method: "POST" },
        );
  },
  importOpml(xml: string): Promise<ImportResult> {
    return isTauri()
      ? call("import_opml", { xml })
      : json("/api/opml/import", { method: "POST", body: JSON.stringify({ xml }) });
  },
  exportOpml(): Promise<string> {
    if (isTauri()) return call("export_opml");
    return authedFetch("/api/opml/export", undefined, false).then(async (res) => {
      if (!res.ok) await parseError(res);
      return res.text();
    });
  },
  listSettings(): Promise<Setting[]> {
    return isTauri() ? call("list_settings") : json("/api/settings");
  },
  saveSettings(settings: Setting[]): Promise<void> {
    if (isTauri()) {
      return Promise.all(settings.map((s) => call("set_setting", s))).then(() => undefined);
    }
    return json("/api/settings", {
      method: "PUT",
      body: JSON.stringify({ settings }),
    });
  },
  listSubscriptions(): Promise<SubscriptionStatus[]> {
    return isTauri() ? call("list_subscriptions") : json("/api/subscriptions");
  },
  openSubscriptionLogin(id: string): Promise<string> {
    return isTauri()
      ? call("open_subscription_login", { id })
      : json<{ message: string }>(`/api/subscriptions/${encodeURIComponent(id)}/login`, { method: "POST" }).then(
          (r) => r.message,
        );
  },
  promptDefaults(): Promise<PromptDefault[]> {
    return isTauri() ? call("prompt_defaults") : json("/api/prompts/defaults");
  },
  probeAi(): Promise<AiProbe> {
    return isTauri() ? call("probe_ai") : json("/api/ai/probe");
  },
  loadFullArticle(articleId: number): Promise<Article> {
    return isTauri()
      ? call("load_full_article", { articleId })
      : json(`/api/articles/${articleId}/full`, { method: "POST" });
  },
  summarize(articleId: number): Promise<AiOutput> {
    return isTauri()
      ? call("ai_summarize", { articleId })
      : json(`/api/ai/summarize/${articleId}`, { method: "POST" });
  },
  summarizeTable(articleId: number): Promise<AiOutput> {
    return isTauri()
      ? call("ai_summarize_table", { articleId })
      : json(`/api/ai/summarize-table/${articleId}`, { method: "POST" });
  },
  tags(articleId: number): Promise<AiOutput> {
    return isTauri()
      ? call("ai_tags", { articleId })
      : json(`/api/ai/tags/${articleId}`, { method: "POST" });
  },
  brief(folderId: number): Promise<AiOutput> {
    return isTauri()
      ? call("ai_brief", { folderId })
      : json(`/api/ai/brief/${folderId}`, { method: "POST" });
  },

  listNotes(): Promise<Note[]> {
    return isTauri() ? call("list_notes") : json("/api/notes");
  },
  getNote(id: number): Promise<Note> {
    return isTauri() ? call("get_note", { id }) : json(`/api/notes/${id}`);
  },
  createNote(title: string, body_md = ""): Promise<Note> {
    const req = { title, body_md };
    return isTauri()
      ? call("create_note", { req })
      : json("/api/notes", { method: "POST", body: JSON.stringify(req) });
  },
  updateNote(id: number, patch: { title?: string; body_md?: string }): Promise<Note> {
    return isTauri()
      ? call("update_note", { id, req: patch })
      : json(`/api/notes/${id}`, { method: "PATCH", body: JSON.stringify(patch) });
  },
  deleteNote(id: number): Promise<void> {
    return isTauri()
      ? call("delete_note", { id })
      : json(`/api/notes/${id}`, { method: "DELETE" });
  },
  askAi(req: AskAiRequest): Promise<AskAiResponse> {
    return isTauri()
      ? call("ask_ai", { req })
      : json("/api/ai/ask", { method: "POST", body: JSON.stringify(req) });
  },

  listPromptTemplates(): Promise<PromptTemplate[]> {
    return isTauri()
      ? call("list_prompt_templates")
      : json("/api/prompt-templates");
  },
  createPromptTemplate(name: string, body: string): Promise<PromptTemplate> {
    const req = { name, body };
    return isTauri()
      ? call("create_prompt_template", { req })
      : json("/api/prompt-templates", { method: "POST", body: JSON.stringify(req) });
  },
  updatePromptTemplate(
    id: number,
    patch: { name?: string; body?: string },
  ): Promise<PromptTemplate> {
    return isTauri()
      ? call("update_prompt_template", { id, req: patch })
      : json(`/api/prompt-templates/${id}`, {
          method: "PATCH",
          body: JSON.stringify(patch),
        });
  },
  deletePromptTemplate(id: number): Promise<void> {
    return isTauri()
      ? call("delete_prompt_template", { id })
      : json(`/api/prompt-templates/${id}`, { method: "DELETE" });
  },
};

/** Save bytes to ~/Downloads via Tauri after one-time folder grant. Returns absolute path. */
export async function saveAiExport(filename: string, bytes: Uint8Array): Promise<string> {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  const bytes_base64 = btoa(binary);
  return call<string>("save_ai_export", { filename, bytesBase64: bytes_base64 });
}

export async function openExternal(url: string) {
  if (isTauri()) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}
