import { t, useLang } from "@/lib/i18n";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  BookOpen,
  FolderPlus,
  Moon,
  Plus,
  RefreshCw,
  Search,
  Settings as SettingsIcon,
  Sun,
  Upload,
} from "lucide-react";
import { Sidebar } from "@/components/Sidebar";
import { PromptTemplatesControls } from "@/components/PromptTemplates";
import { ArticleList } from "@/components/ArticleList";
import { Reader } from "@/components/Reader";
import { AddFeedDialog, FolderDialog, ImportOpmlDialog } from "@/components/Dialogs";
import { KeyboardHelp, SettingsDialog } from "@/components/SettingsDialog";
import { NotebookPanel } from "@/components/Notebook";
import { Button } from "@/components/ui/button";
import { ResizeHandle, usePersistedSize } from "@/components/ui/resizable";
import { Input } from "@/components/ui/input";
import { api, openExternal } from "@/lib/api";
import type {
  ApiError,
  Article,
  ArticleFilter,
  ArticleListItem,
  AskAiContext,
  AskAiResponse,
  Bootstrap,
  FolderNode,
  Selection,
  Setting,
} from "@/lib/types";
import { settingMap } from "@/lib/utils";
import { keyAction } from "@/lib/keys";

function errText(e: unknown): string {
  const a = e as ApiError;
  let msg = a?.hint || a?.error || (e instanceof Error ? e.message : String(e));
  // Unwrap accidental stringify of {"error":"...","code":"error"} into Turkish prose.
  if (typeof msg === "string") {
    const t = msg.trim();
    if (t.startsWith("{") && t.includes('"error"')) {
      try {
        const p = JSON.parse(t) as ApiError;
        if (p?.hint) return p.hint;
        if (typeof p?.error === "string" && p.error.trim()) return p.error;
      } catch {
        /* keep msg */
      }
    }
  }
  return String(msg);
}

function isAiCommand(q: string): boolean {
  return q.startsWith("/");
}

function articleSearchQuery(q: string): string {
  return isAiCommand(q) ? "" : q;
}

function filterFrom(selection: Selection, unreadOnly: boolean, query: string): ArticleFilter {
  const q = query.trim() || undefined;
  const base: ArticleFilter = { unread_only: unreadOnly && selection.kind !== "saved", query: q, limit: 250 };
  if (selection.kind === "saved") return { ...base, saved_only: true, unread_only: false };
  if (selection.kind === "unread") return { ...base, unread_only: true };
  if (selection.kind === "folder") return { ...base, folder_id: selection.id };
  if (selection.kind === "feed") return { ...base, feed_id: selection.id };
  return { ...base, unread_only: unreadOnly };
}

function selectionLabel(s: Selection): string {
  if (s.kind === "all") return "All articles";
  if (s.kind === "unread") return "Unread";
  if (s.kind === "saved") return "Starred / Read later";
  return s.name;
}

function buildAskContext(
  selection: Selection,
  articleId: number | null,
  filterQuery: string,
): AskAiContext {
  const base: AskAiContext = {
    article_id: articleId,
    query: filterQuery.trim() || null,
    selection_kind: selection.kind,
  };
  if (selection.kind === "folder") {
    base.folder_id = selection.id;
    base.selection_label = selection.name;
  } else if (selection.kind === "feed") {
    base.feed_id = selection.id;
    base.selection_label = selection.name;
  } else {
    base.selection_label = selection.kind;
  }
  return base;
}

export default function App() {
  // Re-render the whole tree when the interface language changes, and tell the backend so
  // built-in AI prompts and answers follow the interface language.
  const uiLang = useLang();
  useEffect(() => {
    api.saveSettings([{ key: "ui_lang", value: uiLang }]).catch(() => {});
  }, [uiLang]);
  const [boot, setBoot] = useState<Bootstrap | null>(null);
  const [bootError, setBootError] = useState<string | null>(null);
  const [selection, setSelection] = useState<Selection>({ kind: "unread" });
  const [items, setItems] = useState<ArticleListItem[]>([]);
  const [listLoading, setListLoading] = useState(false);
  const [listError, setListError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [article, setArticle] = useState<Article | null>(null);
  const [articleLoading, setArticleLoading] = useState(false);
  const [query, setQuery] = useState("");
  const [unreadOnly, setUnreadOnly] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [ai, setAi] = useState<AskAiResponse | null>(null);
  const [aiBusy, setAiBusy] = useState(false);
  const [aiError, setAiError] = useState<string | null>(null);
  const [fullBusy, setFullBusy] = useState(false);
  const [fullError, setFullError] = useState<string | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [addBusy, setAddBusy] = useState(false);
  const [addError, setAddError] = useState<string | null>(null);
  const [importOpen, setImportOpen] = useState(false);
  const [importBusy, setImportBusy] = useState(false);
  const [importMsg, setImportMsg] = useState<string | null>(null);
  const [folderOpen, setFolderOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const [notebookOpen, setNotebookOpen] = useState(false);
  const [notesEpoch, setNotesEpoch] = useState(0);
  const [sidebarW, setSidebarW] = usePersistedSize("feedcairn.pane.sidebar", 250, 160, 420);
  const [listW, setListW] = usePersistedSize("feedcairn.pane.list", 340, 220, 560);
  // Reader pane is flex:1 (remaining space); mark key for layout contract.
  useEffect(() => {
    try {
      if (localStorage.getItem("feedcairn.pane.reader") == null) {
        localStorage.setItem("feedcairn.pane.reader", "flex");
      }
    } catch {
      /* ignore */
    }
  }, []);
  const searchRef = useRef<HTMLInputElement>(null);
  const selectedIdRef = useRef<number | null>(null);
  selectedIdRef.current = selectedId;
  const selectionRef = useRef(selection);
  selectionRef.current = selection;
  const queryRef = useRef(query);
  queryRef.current = query;

  const settings = boot?.settings ?? [];
  const map = settingMap(settings);
  const theme = map.theme === "light" ? "light" : "dark";
  const markOnOpen = map.mark_read_on_open !== "false";
  const aiMode = isAiCommand(query);
  const filterQuery = articleSearchQuery(query);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  const reloadBoot = useCallback(async () => {
    const b = await api.bootstrap();
    setBoot(b);
    return b;
  }, []);

  const loadList = useCallback(
    async (sel: Selection, unread: boolean, q: string) => {
      setListLoading(true);
      setListError(null);
      try {
        const rows = await api.listArticles(filterFrom(sel, unread, q));
        setItems(rows);
        return rows;
      } catch (e) {
        setListError(errText(e));
        setItems([]);
        return [];
      } finally {
        setListLoading(false);
      }
    },
    [],
  );

  useEffect(() => {
    reloadBoot()
      .then((b) => {
        const unreadDefault = settingMap(b.settings).unread_only_default !== "false";
        setUnreadOnly(unreadDefault);
        return loadList({ kind: "unread" }, unreadDefault, "");
      })
      .catch((e) => setBootError(errText(e)));
  }, [loadList, reloadBoot]);

  useEffect(() => {
    const q = articleSearchQuery(query);
    const delay = q ? 180 : 0;
    const t = setTimeout(() => {
      void loadList(selection, unreadOnly, q);
    }, delay);
    return () => clearTimeout(t);
  }, [selection, unreadOnly, query, loadList]);

  async function openArticle(id: number, rows?: ArticleListItem[]) {
    setSelectedId(id);
    setAi(null);
    setAiError(null);
    setFullError(null);
    setArticleLoading(true);
    try {
      const full = await api.getArticle(id);
      if (markOnOpen && !full.is_read) {
        const updated = await api.setRead(id, true);
        setArticle(updated);
        const list = rows ?? items;
        setItems(list.map((r) => (r.id === id ? { ...r, is_read: true } : r)));
        await reloadBoot();
      } else {
        setArticle(full);
      }
    } catch (e) {
      setListError(errText(e));
    } finally {
      setArticleLoading(false);
    }
  }

  async function refresh() {
    setRefreshing(true);
    setStatus("Refreshing feeds…");
    try {
      const folderId = selection.kind === "folder" ? selection.id : null;
      const feedId = selection.kind === "feed" ? selection.id : null;
      const result = feedId
        ? { results: [await api.refreshFeed(feedId)], feeds_ok: 1, feeds_failed: 0 }
        : await api.refreshAll(folderId);
      const inserted = result.results.reduce((a, r) => a + r.inserted, 0);
      const failed = result.results.filter((r) => r.error).length;
      setStatus(
        failed
          ? `Fetched ${inserted} new articles · ${failed} feed${failed === 1 ? "" : "s"} failed`
          : `Fetched ${inserted} new article${inserted === 1 ? "" : "s"}`,
      );
      await reloadBoot();
      await loadList(selection, unreadOnly, articleSearchQuery(query));
    } catch (e) {
      setStatus(errText(e));
    } finally {
      setRefreshing(false);
    }
  }

  async function toggleRead() {
    if (!article) return;
    const updated = await api.setRead(article.id, !article.is_read);
    setArticle(updated);
    setItems((rows) => rows.map((r) => (r.id === updated.id ? { ...r, is_read: updated.is_read } : r)));
    await reloadBoot();
  }

  async function toggleSaved() {
    if (!article) return;
    const updated = await api.setSaved(article.id, !article.is_saved);
    setArticle(updated);
    setItems((rows) => rows.map((r) => (r.id === updated.id ? { ...r, is_saved: updated.is_saved } : r)));
    await reloadBoot();
  }

  async function loadFullArticle() {
    if (!article?.url) return;
    setFullBusy(true);
    setFullError(null);
    try {
      const prevHtml = article.content_html;
      const updated = await api.loadFullArticle(article.id);
      setArticle(updated);
      if (updated.content_html === prevHtml) {
        setStatus(
          t("fullTextNoLonger"),
        );
      } else if (updated.content_html.length > prevHtml.length) {
        setStatus(t("fullTextRestored"));
      } else {
        setStatus(t("fullTextLoaded"));
      }
    } catch (e) {
      setFullError(errText(e));
    } finally {
      setFullBusy(false);
    }
  }

  async function runSummarize() {
    if (!article) return;
    setAi(null);
    setAiError(null);
    setAiBusy(true);
    try {
      const out = await api.summarize(article.id);
      setAi({
        provider: out.provider,
        command: out.command ? `${t("cmdSummary")} · ${out.command}` : t("cmdSummary"),
        output: out.output,
        tool_results: [],
      });
    } catch (e) {
      setAiError(errText(e));
    } finally {
      setAiBusy(false);
    }
  }

  /** Summarize current article as a GFM markdown table (previewed as HTML <table>). */
  async function runSummarizeTable() {
    if (!article) return;
    setAi(null);
    setAiError(null);
    setAiBusy(true);
    try {
      const out = await api.summarizeTable(article.id);
      setAi({
        provider: out.provider,
        command: out.command ? `${t("cmdTableSummary")} · ${out.command}` : t("cmdTableSummary"),
        output: out.output,
        tool_results: [],
      });
    } catch (e) {
      setAiError(errText(e));
    } finally {
      setAiBusy(false);
    }
  }

  function moveSelection(delta: number) {
    if (items.length === 0) return;
    const idx = items.findIndex((i) => i.id === selectedIdRef.current);
    const next = idx < 0 ? (delta > 0 ? 0 : items.length - 1) : Math.min(items.length - 1, Math.max(0, idx + delta));
    void openArticle(items[next].id);
  }

  async function runAskAi(prompt: string) {
    const trimmed = prompt.trim();
    if (!trimmed) {
      setAiError(t("emptyAiInstruction"));
      return;
    }
    setAi(null);
    setAiError(null);
    setAiBusy(true);
    try {
      const res = await api.askAi({
        prompt: trimmed,
        context: buildAskContext(selectionRef.current, selectedIdRef.current, ""),
      });
      setAi(res);
      if (res.tool_results?.some((t) => t.ok && /note/i.test(t.name))) {
        setNotesEpoch((n) => n + 1);
        setStatus("AI finished — Notebook refreshed if a note was written");
      }
    } catch (e) {
      setAiError(errText(e));
    } finally {
      setAiBusy(false);
    }
  }

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const tag = (e.target as HTMLElement)?.tagName;
      const typing = tag === "INPUT" || tag === "TEXTAREA" || (e.target as HTMLElement)?.isContentEditable;
      const action = keyAction(e.key, typing);
      if (!action) return;
      e.preventDefault();
      if (action === "search") {
        const el = searchRef.current;
        el?.focus();
        if (!queryRef.current) {
          setQuery("/");
          // place caret after /
          requestAnimationFrame(() => {
            el?.setSelectionRange(1, 1);
          });
        }
      } else if (action === "help") setHelpOpen(true);
      else if (action === "next") moveSelection(1);
      else if (action === "prev") moveSelection(-1);
      else if (action === "read") void toggleRead();
      else if (action === "star") void toggleSaved();
      else if (action === "refresh") void refresh();
      else if (action === "open" && article?.url) void openExternal(article.url);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const emptyLabel = useMemo(() => {
    if (filterQuery.trim()) return "No articles match that search.";
    if (selection.kind === "saved") return "Nothing starred yet. Press s to star / read later.";
    if (unreadOnly || selection.kind === "unread") return "You're all caught up.";
    return "No articles in this view. Refresh feeds or import an OPML file.";
  }, [filterQuery, selection.kind, unreadOnly]);

  async function saveSettings(next: Setting[]) {
    await api.saveSettings(next);
    const b = await reloadBoot();
    document.documentElement.dataset.theme = settingMap(b.settings).theme === "light" ? "light" : "dark";
  }

  if (bootError) {
    return (
      <div className="flex h-full items-center justify-center p-8 text-center">
        <div>
          <h1 className="font-[family-name:var(--font-serif)] text-2xl">Feedcairn couldn’t open</h1>
          <p className="mt-2 text-sm text-[var(--danger)]">{bootError}</p>
          <p className="mt-3 text-sm text-[var(--muted)]">
            Start the API with <code>npm run web</code> or the desktop shell with <code>npm run tauri dev</code>.
          </p>
        </div>
      </div>
    );
  }

  if (!boot) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-[var(--muted)]">
        Opening your library…
      </div>
    );
  }

  function flattenFolders(nodes: FolderNode[]): { id: number; name: string }[] {
    return nodes.flatMap((n) => [
      { id: n.folder.id, name: n.folder.name },
      ...flattenFolders(n.children),
    ]);
  }

  const showReaderPane = !!(article || aiBusy || ai || aiError);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex h-12 shrink-0 items-center gap-2 border-b border-[var(--line)] bg-[var(--sidebar)] px-3">
        <img src="/feedcairn.svg" alt="" className="h-6 w-6" />
        <div className="hidden leading-tight sm:block">
          <div className="text-sm font-semibold tracking-tight">Feedcairn</div>
          <div className="text-[11px] text-[var(--muted)]">{selectionLabel(selection)}</div>
        </div>
        <div className="relative mx-2 min-w-0 flex-1">
          <Search
            className={`pointer-events-none absolute left-2 top-2 h-4 w-4 ${
              aiMode ? "text-[var(--accent)]" : "text-[var(--muted)]"
            }`}
          />
          <Input
            ref={searchRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape" && isAiCommand(query)) {
                e.preventDefault();
                setQuery("");
                return;
              }
              if (e.key === "Enter" && isAiCommand(query)) {
                e.preventDefault();
                void runAskAi(query.slice(1));
              }
            }}
            placeholder={aiMode ? t("searchPlaceholderAi") : t("searchPlaceholder")}
            className={`pl-8 ${
              aiMode
                ? "border-[var(--accent)] ring-1 ring-[color-mix(in_srgb,var(--accent)_35%,transparent)]"
                : ""
            }`}
            title={
              aiMode
                ? t("searchTitleAi")
                : t("searchTitle")
            }
          />
        </div>
        <PromptTemplatesControls
          query={query}
          aiMode={aiMode}
          onFill={(slashPrompt) => {
            setQuery(slashPrompt);
            requestAnimationFrame(() => searchRef.current?.focus());
          }}
          onRun={(slashPrompt) => {
            setQuery(slashPrompt);
            void runAskAi(slashPrompt.slice(1));
          }}
        />
        <Button
          variant={unreadOnly ? "default" : "outline"}
          size="sm"
          onClick={() => setUnreadOnly((v) => !v)}
          title="Unread only"
        >
          Unread
        </Button>
        <Button variant="outline" size="sm" onClick={() => setUnreadOnly(false)}>
          All
        </Button>
        <Button variant="ghost" size="sm" disabled title="Advanced filters are planned (P1)">
          Filters
        </Button>
        <Button variant="ghost" size="icon" onClick={() => void refresh()} disabled={refreshing} title="Refresh (r)">
          <RefreshCw className={`h-4 w-4 ${refreshing ? "animate-spin" : ""}`} />
        </Button>
        <Button variant="ghost" size="icon" onClick={() => setAddOpen(true)} title="Add feed">
          <Plus className="h-4 w-4" />
        </Button>
        <Button variant="ghost" size="icon" onClick={() => setFolderOpen(true)} title="New folder">
          <FolderPlus className="h-4 w-4" />
        </Button>
        <Button variant="ghost" size="icon" onClick={() => setImportOpen(true)} title="Import OPML">
          <Upload className="h-4 w-4" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          onClick={() => void saveSettings([{ key: "theme", value: theme === "dark" ? "light" : "dark" }])}
          title="Toggle theme"
        >
          {theme === "dark" ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
        </Button>
        <Button
          variant={notebookOpen ? "default" : "ghost"}
          size="icon"
          onClick={() => setNotebookOpen((v) => !v)}
          title="Notebook"
        >
          <BookOpen className="h-4 w-4" />
        </Button>
        <Button variant="ghost" size="icon" onClick={() => setSettingsOpen(true)} title="Settings">
          <SettingsIcon className="h-4 w-4" />
        </Button>
      </header>
      {status ? (
        <div className="border-b border-[var(--line)] bg-[var(--bg-elevated)] px-4 py-1.5 text-xs text-[var(--muted)]">
          {status}
        </div>
      ) : null}
      <div className="flex min-h-0 flex-1">
        <aside
          className="hidden shrink-0 border-r border-[var(--line)] bg-[var(--sidebar)] md:block"
          style={{ width: sidebarW }}
        >
          <Sidebar
            folders={boot.folders}
            rootFeeds={boot.root_feeds}
            counts={boot.counts}
            selection={selection}
            onSelect={(s) => {
              setSelection(s);
              setSelectedId(null);
              setArticle(null);
              setAi(null);
              setAiError(null);
            }}
            onMoveFeed={async (feedId, folderId) => {
              if (folderId == null) await api.updateFeed(feedId, { unfile: true });
              else await api.updateFeed(feedId, { folder_id: folderId });
              await reloadBoot();
              setStatus(folderId == null ? t("feedUnfiled") : t("feedMoved"));
            }}
            onRenameFeed={async (feedId, currentName) => {
              const title = window.prompt(t("newFeedName"), currentName);
              if (!title?.trim() || title.trim() === currentName) return;
              await api.updateFeed(feedId, { title: title.trim() });
              if (selection.kind === "feed" && selection.id === feedId) {
                setSelection({ ...selection, name: title.trim() });
              }
              await reloadBoot();
            }}
            onDeleteFeed={async (feedId, name) => {
              if (!window.confirm(t("deleteFeedConfirm", { name }))) return;
              await api.deleteFeed(feedId);
              if (selection.kind === "feed" && selection.id === feedId) {
                setSelection({ kind: "unread" });
              }
              await reloadBoot();
            }}
            onRenameFolder={async (folderId, currentName) => {
              const name = window.prompt(t("newFolderName"), currentName);
              if (!name?.trim() || name.trim() === currentName) return;
              await api.renameFolder(folderId, name.trim());
              if (selection.kind === "folder" && selection.id === folderId) {
                setSelection({ ...selection, name: name.trim() });
              }
              await reloadBoot();
            }}
            onDeleteFolder={async (folderId, name) => {
              if (!window.confirm(t("deleteFolderConfirm", { name }))) return;
              await api.deleteFolder(folderId);
              if (selection.kind === "folder" && selection.id === folderId) {
                setSelection({ kind: "unread" });
              }
              await reloadBoot();
            }}
          />
        </aside>
        <ResizeHandle
          direction="ew"
          value={sidebarW}
          onChange={setSidebarW}
          min={160}
          max={420}
          className="hidden md:block"
          title="Resize sidebar"
        />
        <div className="flex min-w-0 flex-1">
          <div
            className={`min-w-0 max-md:!w-full md:shrink-0 ${
              showReaderPane ? "hidden md:block" : "block"
            }`}
            style={{ width: listW }}
          >
            <div className="border-b border-[var(--line)] p-2 md:hidden">
              <select
                className="h-8 w-full rounded-md border border-[var(--line)] bg-[var(--bg-elevated)] px-2 text-sm"
                value={
                  selection.kind === "folder"
                    ? `folder:${selection.id}`
                    : selection.kind === "feed"
                      ? `feed:${selection.id}`
                      : selection.kind
                }
                onChange={(e) => {
                  const v = e.target.value;
                  if (v === "unread" || v === "all" || v === "saved") setSelection({ kind: v });
                }}
              >
                <option value="unread">Unread</option>
                <option value="all">All articles</option>
                <option value="saved">Starred / Read later</option>
              </select>
            </div>
            <ArticleList
              items={items}
              selectedId={selectedId}
              loading={listLoading}
              error={listError}
              emptyLabel={emptyLabel}
              onSelect={(id) => void openArticle(id)}
              onMarkAllRead={async () => {
                await api.markAllRead(filterFrom(selection, unreadOnly, filterQuery));
                await reloadBoot();
                await loadList(selection, unreadOnly, filterQuery);
                if (article) setArticle({ ...article, is_read: true });
              }}
            />
          </div>
          <ResizeHandle
            direction="ew"
            value={listW}
            onChange={setListW}
            min={220}
            max={560}
            className="hidden md:block"
            title="Resize article list"
          />
          <div
            className={`min-w-0 flex-1 bg-[var(--bg-elevated)] ${
              showReaderPane ? "block" : "hidden md:block"
            }`}
            data-pane="reader"
          >
            <div className="border-b border-[var(--line)] px-2 py-1 md:hidden">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  setArticle(null);
                  setSelectedId(null);
                  setAi(null);
                  setAiError(null);
                }}
              >
                ← Back to list
              </Button>
            </div>
            <Reader
              article={article}
              loading={articleLoading}
              aiBusy={aiBusy}
              ai={ai}
              aiError={aiError}
              onToggleRead={() => void toggleRead()}
              onToggleSaved={() => void toggleSaved()}
              onOpenLink={() => article?.url && void openExternal(article.url)}
              onLoadFull={() => void loadFullArticle()}
              onSummarize={() => void runSummarize()}
              onSummarizeTable={() => void runSummarizeTable()}
              onFeedSubscribed={async () => {
                await reloadBoot();
                setStatus(t("feedSubscribedRefreshed"));
              }}
              fullBusy={fullBusy}
              fullError={fullError}
            />
          </div>
        </div>
        <NotebookPanel open={notebookOpen} onClose={() => setNotebookOpen(false)} refreshToken={notesEpoch} />
      </div>
      {(selection.kind === "feed" || selection.kind === "folder") && (
        <div className="flex flex-wrap items-center gap-2 border-t border-[var(--line)] bg-[var(--sidebar)] px-3 py-2 text-xs">
          {selection.kind === "feed" ? (
            <>
              <span className="text-[var(--muted)]">{t("rename")}</span>
              <Input
                className="h-7 max-w-[16rem]"
                defaultValue={selection.name}
                key={selection.id}
                onBlur={async (e) => {
                  const title = e.target.value.trim();
                  if (title && title !== selection.name) {
                    await api.updateFeed(selection.id, { title });
                    setSelection({ ...selection, name: title });
                    await reloadBoot();
                  }
                }}
              />
              <select
                className="h-7 rounded-md border border-[var(--line)] bg-[var(--bg-elevated)] px-2"
                defaultValue=""
                onChange={async (e) => {
                  const v = e.target.value;
                  if (v === "") return;
                  if (v === "unfile") await api.updateFeed(selection.id, { unfile: true });
                  else await api.updateFeed(selection.id, { folder_id: Number(v) });
                  await reloadBoot();
                }}
              >
                <option value="">{t("moveToFolder")}</option>
                <option value="unfile">{t("unfiled")}</option>
                {flattenFolders(boot.folders).map((f) => (
                  <option key={f.id} value={f.id}>
                    {f.name}
                  </option>
                ))}
              </select>
              <Button
                variant="danger"
                size="sm"
                onClick={async () => {
                  if (!confirm(`Unsubscribe from ${selection.name}?`)) return;
                  await api.deleteFeed(selection.id);
                  setSelection({ kind: "unread" });
                  await reloadBoot();
                }}
              >
                Unsubscribe
              </Button>
            </>
          ) : (
            <>
              <span className="text-[var(--muted)]">{t("renameFolder")}</span>
              <Input
                className="h-7 max-w-[16rem]"
                defaultValue={selection.name}
                key={selection.id}
                onBlur={async (e) => {
                  const name = e.target.value.trim();
                  if (name && name !== selection.name) {
                    await api.renameFolder(selection.id, name);
                    setSelection({ ...selection, name });
                    await reloadBoot();
                  }
                }}
              />
              <span className="text-[var(--muted)]">
                {t("folderSummary")} <code className="rounded bg-[var(--line)] px-1">{t("folderSummaryCmd")}</code>
              </span>
              <Button
                variant="danger"
                size="sm"
                onClick={async () => {
                  if (!confirm(`Delete folder ${selection.name}? Feeds will be unfiled.`)) return;
                  await api.deleteFolder(selection.id);
                  setSelection({ kind: "unread" });
                  await reloadBoot();
                }}
              >
                Delete folder
              </Button>
            </>
          )}
        </div>
      )}
      <AddFeedDialog
        open={addOpen}
        onOpenChange={setAddOpen}
        folders={boot.folders}
        busy={addBusy}
        error={addError}
        onSubmit={async (url, folderId) => {
          setAddBusy(true);
          setAddError(null);
          try {
            const feed = await api.addFeed(url, folderId);
            await reloadBoot();
            setSelection({ kind: "feed", id: feed.id, name: feed.title });
            setAddOpen(false);
            setStatus(`Subscribed to ${feed.title}`);
          } catch (e) {
            setAddError(errText(e));
          } finally {
            setAddBusy(false);
          }
        }}
      />
      <ImportOpmlDialog
        open={importOpen}
        onOpenChange={setImportOpen}
        busy={importBusy}
        message={importMsg}
        onExport={async () => {
          const xml = await api.exportOpml();
          const blob = new Blob([xml], { type: "text/xml" });
          const url = URL.createObjectURL(blob);
          const a = document.createElement("a");
          a.href = url;
          a.download = "feedcairn.opml";
          a.click();
          URL.revokeObjectURL(url);
        }}
        onImport={async (xml) => {
          setImportBusy(true);
          setImportMsg(null);
          try {
            const r = await api.importOpml(xml);
            setImportMsg(
              `Imported ${r.feeds_added} feeds and ${r.folders_created} folders (${r.feeds_skipped} already present). Refresh to fetch articles.`,
            );
            await reloadBoot();
          } catch (e) {
            setImportMsg(errText(e));
          } finally {
            setImportBusy(false);
          }
        }}
      />
      <FolderDialog
        open={folderOpen}
        onOpenChange={setFolderOpen}
        onSubmit={async (name) => {
          await api.createFolder(name);
          await reloadBoot();
          setFolderOpen(false);
        }}
      />
      <SettingsDialog
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        settings={settings}
        onSave={saveSettings}
      />
      <KeyboardHelp open={helpOpen} onOpenChange={setHelpOpen} />
    </div>
  );
}
