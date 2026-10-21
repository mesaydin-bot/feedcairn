import { t } from "@/lib/i18n";
import { useEffect, useMemo, useRef, useState, type CSSProperties, type RefObject } from "react";
import {
  Circle,
  Download,
  ExternalLink,
  FileText,
  Minus,
  Plus,
  Sparkles,
  Star,
  Table2,
  Type,
} from "lucide-react";
import type { AskAiResponse, Article } from "@/lib/types";
import { fullDate } from "@/lib/time";
import { Button } from "@/components/ui/button";
import { api, openExternal } from "@/lib/api";
import { splitAiAnswer, stripToolArtifacts } from "@/lib/aiMarkdown";
import { AiArtifactFrame } from "@/components/AiArtifactFrame";
import { parseSearchRssHits, searchFeedUrlFromTools } from "@/lib/searchRss";
import {
  downloadAiMarkdown,
  downloadAiPdf,
  downloadArticleMarkdown,
  downloadArticlePdf,
  downloadCombinedMarkdown,
  downloadCombinedPdf,
} from "@/lib/aiExport";
import {
  FONT_SIZES,
  READER_FONTS,
  useReaderTypography,
  type ReaderFontId,
  type ReaderFontSize,
} from "@/lib/readerTypography";

function aiTitle(ai: AskAiResponse): string {
  if (/tablo|table/i.test(ai.command || "")) return t("tableSummary");
  if (/özet|summar/i.test(ai.command || "")) return t("summary");
  return t("aiAnswer");
}

/** Prefer cleaned output; if strip leaves empty, fall back to summarize/analyze tool prose. */
function readerDisplayText(ai: AskAiResponse): string {
  const raw = ai.output || "";
  // Never treat backend error JSON as an AI answer body.
  const trimmed = raw.trim();
  if (trimmed.startsWith("{") && /"error"\s*:/.test(trimmed) && /"code"\s*:/.test(trimmed)) {
    try {
      const p = JSON.parse(trimmed) as { error?: string; hint?: string };
      if (p?.hint) return p.hint;
      if (typeof p?.error === "string" && p.error.trim()) return `**${t("errorLabel")}:** ${p.error}`;
    } catch {
      /* fall through */
    }
  }
  if (stripToolArtifacts(raw).trim()) return raw;
  const fromTools = (ai.tool_results || [])
    .filter(
      (t) =>
        t.ok &&
        /summarize|analyze|suggest|list_notes/i.test(t.name) &&
        (t.detail || "").trim().length > 40 &&
        !/fast-path article_id=/i.test(t.detail || ""),
    )
    .map((t) => t.detail.trim());
  if (fromTools.length) return fromTools.join("\n\n");
  // search_rss_web is rendered as Subscribe cards; keep a short lead-in in the UI language if prose is empty.
  const web = (ai.tool_results || []).find((t) => t.ok && t.name === "search_rss_web");
  if (web) {
    const n = (web.detail.match(/---ITEM---/g) || []).length;
    if (n > 0) {
      return t("rssResults", { n });
    }
    return t("rssNoResults");
  }
  return raw;
}

function AiAnswerBlock({
  aiBusy,
  ai,
  aiError,
  bodyRef,
  onFeedSubscribed,
}: {
  aiBusy: boolean;
  ai: AskAiResponse | null;
  aiError: string | null;
  bodyRef?: RefObject<HTMLDivElement | null>;
  onFeedSubscribed?: () => void | Promise<void>;
}) {
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [subStatus, setSubStatus] = useState<string | null>(null);
  const [subBusyUrl, setSubBusyUrl] = useState<string | null>(null);
  const [subscribedUrls, setSubscribedUrls] = useState<Record<string, true>>({});
  const displayText = useMemo(() => (ai ? readerDisplayText(ai) : ""), [ai]);
  const split = useMemo(() => (displayText ? splitAiAnswer(displayText) : null), [displayText]);
  const hasVisual = !!(
    split &&
    split.blocks.some((b) => b.kind === "artifact" || b.kind === "mermaid")
  );
  const searchHits = useMemo(() => parseSearchRssHits(ai?.tool_results), [ai]);
  const searchFeedUrl = useMemo(() => searchFeedUrlFromTools(ai?.tool_results), [ai]);

  useEffect(() => {
    setSubStatus(null);
    setSubBusyUrl(null);
    setSubscribedUrls({});
  }, [ai?.output, ai?.tool_results]);

  // Render mermaid diagrams after AI HTML mounts (fail soft → leave source visible).
  useEffect(() => {
    if (!split) return;
    const root = bodyRef?.current;
    if (!root) return;
    let cancelled = false;
    (async () => {
      try {
        const nodes = root.querySelectorAll<HTMLElement>(
          ".mermaid, pre code.language-mermaid, code.language-mermaid",
        );
        if (!nodes.length || cancelled) return;
        const targets: HTMLElement[] = [];
        nodes.forEach((el) => {
          if (el.classList.contains("mermaid") && el.tagName !== "CODE") {
            targets.push(el);
            return;
          }
          const src = el.textContent || "";
          const wrap = document.createElement("div");
          wrap.className = "mermaid";
          wrap.textContent = src;
          const pre = el.closest("pre") || el;
          pre.replaceWith(wrap);
          targets.push(wrap);
        });
        if (cancelled || !targets.length) return;
        const mermaid = (await import("mermaid")).default;
        mermaid.initialize({
          startOnLoad: false,
          theme: "dark",
          securityLevel: "strict",
          fontFamily: "inherit",
        });
        await mermaid.run({ nodes: targets });
      } catch (err) {
        console.warn("Feedcairn: mermaid render failed", err);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [split, bodyRef]);

  async function subscribeFeed(url: string, label?: string) {
    if (!url || subscribedUrls[url]) return;
    setSubBusyUrl(url);
    setSubStatus(null);
    try {
      const feed = await api.addFeed(url, null, label || undefined);
      setSubscribedUrls((prev) => ({ ...prev, [url]: true }));
      setSubStatus(t("subscribedTo", { name: feed.title || label || url }));
      if (onFeedSubscribed) await onFeedSubscribed();
    } catch (e) {
      setSubStatus(e instanceof Error ? e.message : String(e));
    } finally {
      setSubBusyUrl(null);
    }
  }

  return (
    <>
      {aiBusy ? (
        <div className="article-body mt-6 text-[var(--muted)]">
          {t("aiRunning")}
        </div>
      ) : null}
      {aiError ? (
        <div className="mt-6 max-w-[42rem] whitespace-pre-wrap rounded-lg border border-[var(--danger)]/40 bg-[color-mix(in_srgb,var(--danger)_8%,transparent)] px-4 py-3 text-sm leading-relaxed">
          {aiError}
        </div>
      ) : null}
      {ai ? (
        <section className={hasVisual ? "mt-6 max-w-[min(100%,56rem)]" : "mt-6 max-w-[42rem]"}>
          <div className="mb-2 flex flex-wrap items-center gap-2">
            <p className="text-[11px] font-medium uppercase tracking-wide text-[var(--muted)]">
              {aiTitle(ai)}
            </p>
          </div>
          {split && split.blocks.length ? (
            <div
              ref={bodyRef}
              className={`article-body ai-answer-body${hasVisual ? " ai-answer-body--wide" : ""}`}
              onClick={(e) => {
                const a = (e.target as HTMLElement).closest("a");
                if (a?.href) {
                  e.preventDefault();
                  void openExternal(a.href);
                }
              }}
            >
              {split.blocks.map((b, i) => {
                if (b.kind === "prose") {
                  return (
                    <div
                      key={`prose-${i}`}
                      className="ai-prose-block"
                      dangerouslySetInnerHTML={{ __html: b.html }}
                    />
                  );
                }
                if (b.kind === "artifact") {
                  const raw = split.artifacts.get(b.id) || "";
                  return <AiArtifactFrame key={b.id} artifactId={b.id} html={raw} />;
                }
                return (
                  <div key={b.id} className="mermaid">
                    {b.source}
                  </div>
                );
              })}
            </div>
          ) : (
            <p className="article-body text-[var(--muted)]">
              {t("aiEmpty")}
            </p>
          )}
          {ai.provider ? (
            <p className="mt-3 text-[11px] text-[var(--muted)] opacity-70">
              {ai.provider || ""}
            </p>
          ) : null}
          {searchHits.length || searchFeedUrl ? (
            <div className="mt-4 space-y-3">
              {searchFeedUrl ? (
                <div className="flex flex-wrap items-center gap-2 rounded-lg border border-[var(--line)] bg-[var(--sidebar)]/50 px-3 py-2">
                  <p className="text-xs text-[var(--muted)]">
                    {t("googleNewsFeed")}
                  </p>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={!!subscribedUrls[searchFeedUrl] || subBusyUrl === searchFeedUrl}
                    onClick={() => void subscribeFeed(searchFeedUrl, "Google News search")}
                  >
                    {subscribedUrls[searchFeedUrl]
                      ? t("subscribed")
                      : subBusyUrl === searchFeedUrl
                        ? t("subscribing")
                        : t("subscribe")}
                  </Button>
                </div>
              ) : null}
              {searchHits.map((h, i) => (
                <div
                  key={`${h.link}-${i}`}
                  className="rounded-lg border border-[var(--line)] bg-[var(--bg-elevated)] px-3 py-2.5"
                >
                  <div className="flex flex-wrap items-start justify-between gap-2">
                    <div className="min-w-0 flex-1">
                      {h.link ? (
                        <a
                          href={h.link}
                          className="font-medium text-[var(--text)] hover:underline"
                          onClick={(e) => {
                            e.preventDefault();
                            void openExternal(h.link);
                          }}
                        >
                          {h.title}
                        </a>
                      ) : (
                        <p className="font-medium">{h.title}</p>
                      )}
                      <p className="mt-0.5 text-[11px] text-[var(--muted)]">
                        {[h.source, h.published].filter(Boolean).join(" · ")}
                      </p>
                      {h.summary ? (
                        <p className="mt-1.5 text-sm leading-relaxed text-[var(--text)]/90">{h.summary}</p>
                      ) : null}
                    </div>
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      className="shrink-0"
                      disabled={!!subscribedUrls[h.feedUrl] || subBusyUrl === h.feedUrl}
                      onClick={() => void subscribeFeed(h.feedUrl, h.source || h.title)}
                    >
                      {subscribedUrls[h.feedUrl]
                        ? t("subscribed")
                        : subBusyUrl === h.feedUrl
                          ? "…"
                          : t("subscribe")}
                    </Button>
                  </div>
                </div>
              ))}
              {subStatus ? (
                <p className="text-xs text-[var(--muted)]" role="status">
                  {subStatus}
                </p>
              ) : null}
            </div>
          ) : null}
          {ai.tool_results?.length ? (
            <div className="mt-3">
              <button
                type="button"
                className="text-[11px] text-[var(--muted)] underline-offset-2 hover:underline"
                onClick={() => setDetailsOpen((v) => !v)}
                aria-expanded={detailsOpen}
              >
                {detailsOpen ? t("hideDetails") : t("details")}
              </button>
              {detailsOpen ? (
                <div className="mt-2 space-y-2 border-t border-[var(--line)] pt-3">
                  {ai.tool_results.map((t, i) => (
                    <div key={`${t.name}-${i}`} className="rounded-md bg-[var(--sidebar)]/60 px-3 py-2 text-xs">
                      <span className={t.ok ? "text-[var(--accent)]" : "text-[var(--danger)]"}>
                        {t.ok ? "✓" : "✗"} {t.name}
                      </span>
                      <pre className="mt-1 whitespace-pre-wrap font-sans text-[11px] leading-relaxed text-[var(--muted)]">
                        {t.detail}
                      </pre>
                    </div>
                  ))}
                </div>
              ) : null}
            </div>
          ) : null}
        </section>
      ) : null}
    </>
  );
}


export function Reader({
  article,
  loading,
  aiBusy,
  ai,
  aiError,
  onToggleRead,
  onToggleSaved,
  onOpenLink,
  onLoadFull,
  onSummarize,
  onSummarizeTable,
  onFeedSubscribed,
  fullBusy,
  fullError,
}: {
  article: Article | null;
  loading: boolean;
  aiBusy: boolean;
  ai: AskAiResponse | null;
  aiError: string | null;
  onToggleRead: () => void;
  onToggleSaved: () => void;
  onOpenLink: () => void;
  onLoadFull: () => void;
  onSummarize: () => void;
  onSummarizeTable: () => void;
  onFeedSubscribed?: () => void | Promise<void>;
  fullBusy: boolean;
  fullError: string | null;
}) {
  const typography = useReaderTypography();
  const [typoOpen, setTypoOpen] = useState(false);
  const typoRef = useRef<HTMLDivElement>(null);
  const [dlOpen, setDlOpen] = useState(false);
  const [pdfBusy, setPdfBusy] = useState(false);
  const [dlStatus, setDlStatus] = useState<string | null>(null);
  const dlRef = useRef<HTMLDivElement>(null);
  const aiBodyRef = useRef<HTMLDivElement>(null);
  const articleBodyRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!typoOpen && !dlOpen) return;
    function onDoc(e: MouseEvent) {
      if (typoOpen && typoRef.current && !typoRef.current.contains(e.target as Node)) {
        setTypoOpen(false);
      }
      if (dlOpen && dlRef.current && !dlRef.current.contains(e.target as Node)) {
        setDlOpen(false);
      }
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        setTypoOpen(false);
        setDlOpen(false);
      }
    }
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [typoOpen, dlOpen]);

  function flashStatus(msg: string) {
    setDlStatus(msg);
    window.setTimeout(() => setDlStatus(null), 4000);
  }

  const hasAiAnswer = !!(ai && readerDisplayText(ai).trim());

  async function runExport(
    kind: "ozet-md" | "ozet-pdf" | "haber-md" | "haber-pdf" | "both-md" | "both-pdf",
  ) {
    setDlOpen(false);
    try {
      let path: string | null = null;
      const aiText = ai ? readerDisplayText(ai) : "";
      if (kind === "ozet-md") {
        if (!ai) return;
        path = await downloadAiMarkdown(aiText);
        flashStatus(path ? `Kaydedildi: ${path}` : "MD indirildi");
      } else if (kind === "ozet-pdf") {
        if (!ai) return;
        setPdfBusy(true);
        path = await downloadAiPdf(aiText, aiBodyRef.current);
        flashStatus(path ? `Kaydedildi: ${path}` : "PDF indirildi");
      } else if (kind === "haber-md") {
        if (!article) return;
        path = await downloadArticleMarkdown(article);
        flashStatus(path ? `Kaydedildi: ${path}` : "MD indirildi");
      } else if (kind === "haber-pdf") {
        if (!article) return;
        setPdfBusy(true);
        path = await downloadArticlePdf(article, articleBodyRef.current);
        flashStatus(path ? `Kaydedildi: ${path}` : "PDF indirildi");
      } else if (kind === "both-md") {
        if (!ai || !article) return;
        path = await downloadCombinedMarkdown(aiText, article);
        flashStatus(path ? `Kaydedildi: ${path}` : "MD indirildi");
      } else if (kind === "both-pdf") {
        if (!ai || !article) return;
        setPdfBusy(true);
        path = await downloadCombinedPdf(aiText, article, aiBodyRef.current, articleBodyRef.current);
        flashStatus(path ? `Kaydedildi: ${path}` : "PDF indirildi");
      }
    } catch (e) {
      console.error(e);
      flashStatus(e instanceof Error ? e.message : t("downloadFailed"));
    } finally {
      setPdfBusy(false);
    }
  }

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-[var(--muted)]">
        Opening article…
      </div>
    );
  }

  const hasAi = aiBusy || !!ai || !!aiError;

  if (!article) {
    if (hasAi) {
      return (
        <div className="flex h-full min-h-0 flex-col">
          <div className="border-b border-[var(--line)] px-4 py-2">
            <p className="text-xs font-medium uppercase tracking-wide text-[var(--muted)]">{t("aiAnswer")}</p>
            <h1 className="mt-0.5 font-[family-name:var(--font-serif)] text-xl font-medium">
              {t("fromSearchBar")}
            </h1>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
            <AiAnswerBlock aiBusy={aiBusy} ai={ai} aiError={aiError} bodyRef={aiBodyRef} onFeedSubscribed={onFeedSubscribed} />
            <p className="mt-6 max-w-md text-sm text-[var(--muted)]">
              {t("noArticleSelected")}
            </p>
          </div>
        </div>
      );
    }
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 px-8 text-center">
        <p className="font-[family-name:var(--font-serif)] text-2xl">Nothing selected</p>
        <p className="max-w-sm text-sm text-[var(--muted)]">
          Pick an article from the list, or press <kbd className="rounded bg-[var(--line)] px-1">j</kbd>/
          <kbd className="rounded bg-[var(--line)] px-1">n</kbd> and{" "}
          <kbd className="rounded bg-[var(--line)] px-1">k</kbd>/
          <kbd className="rounded bg-[var(--line)] px-1">p</kbd> to move.
        </p>
        <p className="mt-2 max-w-sm text-sm text-[var(--muted)]">
          {t("aiHowTo")}
        </p>
      </div>
    );
  }

  const readerStyle = {
    ["--reader-font" as string]: typography.fontStack,
    ["--reader-font-size" as string]: `${typography.fontSize}px`,
  } as CSSProperties;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-wrap items-center gap-1 border-b border-[var(--line)] px-4 py-2">
        <Button variant="ghost" size="sm" onClick={onToggleRead}>
          <Circle className={`h-3.5 w-3.5 ${article.is_read ? "" : "fill-[var(--unread)] text-[var(--unread)]"}`} />
          {article.is_read ? "Mark unread" : "Mark read"}
        </Button>
        <Button variant="ghost" size="sm" onClick={onToggleSaved}>
          <Star className={`h-3.5 w-3.5 ${article.is_saved ? "fill-[var(--saved)] text-[var(--saved)]" : ""}`} />
          {article.is_saved ? "Starred" : "Star / later"}
        </Button>
        <Button variant="ghost" size="sm" onClick={onOpenLink} disabled={!article.url} title="Open original (o / v)">
          <ExternalLink className="h-3.5 w-3.5" />
          Original
        </Button>
        {article.url ? (
          <Button
            variant="ghost"
            size="sm"
            onClick={onLoadFull}
            disabled={fullBusy}
            title="Fetch and extract the full article from the original page"
          >
            <FileText className="h-3.5 w-3.5" />
            {fullBusy ? t("loading") : t("loadFullText")}
          </Button>
        ) : null}
        <Button
          variant="ghost"
          size="sm"
          onClick={onSummarize}
          disabled={aiBusy || !article}
          title={t("summarizeTitle")}
        >
          <Sparkles className="h-3.5 w-3.5" />
          {aiBusy ? t("summarizing") : t("summarize")}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={onSummarizeTable}
          disabled={aiBusy || !article}
          title={t("tableSummaryTitle")}
        >
          <Table2 className="h-3.5 w-3.5" />
          {t("tableSummary")}
        </Button>
        <span className="mx-1 h-4 w-px bg-[var(--line)]" />
        <div className="relative" ref={dlRef}>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => {
              setTypoOpen(false);
              setDlOpen((v) => !v);
            }}
            aria-expanded={dlOpen}
            aria-haspopup="menu"
            title={t("downloadTitle")}
            disabled={pdfBusy}
          >
            <Download className="h-3.5 w-3.5" />
            {pdfBusy ? "PDF…" : t("download")}
          </Button>
          {dlOpen ? (
            <div
              className="absolute left-0 top-full z-30 mt-1 min-w-[12.5rem] rounded-lg border border-[var(--line)] bg-[var(--bg-elevated)] py-1 shadow-[var(--shadow)]"
              role="menu"
            >
              <button
                type="button"
                role="menuitem"
                disabled={!hasAiAnswer}
                className="flex w-full items-center px-3 py-1.5 text-left text-sm hover:bg-[color-mix(in_srgb,var(--line)_50%,transparent)] disabled:cursor-not-allowed disabled:opacity-40"
                onClick={() => void runExport("ozet-md")}
              >
                {t("summaryMd")}
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={!hasAiAnswer || pdfBusy}
                className="flex w-full items-center px-3 py-1.5 text-left text-sm hover:bg-[color-mix(in_srgb,var(--line)_50%,transparent)] disabled:cursor-not-allowed disabled:opacity-40"
                onClick={() => void runExport("ozet-pdf")}
              >
                {t("summaryPdf")}
              </button>
              <div className="my-1 border-t border-[var(--line)]" />
              <button
                type="button"
                role="menuitem"
                className="flex w-full items-center px-3 py-1.5 text-left text-sm hover:bg-[color-mix(in_srgb,var(--line)_50%,transparent)]"
                onClick={() => void runExport("haber-md")}
              >
                Haber → MD
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={pdfBusy}
                className="flex w-full items-center px-3 py-1.5 text-left text-sm hover:bg-[color-mix(in_srgb,var(--line)_50%,transparent)] disabled:opacity-40"
                onClick={() => void runExport("haber-pdf")}
              >
                Haber → PDF
              </button>
              <div className="my-1 border-t border-[var(--line)]" />
              <button
                type="button"
                role="menuitem"
                disabled={!hasAiAnswer}
                className="flex w-full items-center px-3 py-1.5 text-left text-sm hover:bg-[color-mix(in_srgb,var(--line)_50%,transparent)] disabled:cursor-not-allowed disabled:opacity-40"
                onClick={() => void runExport("both-md")}
              >
                {t("summaryArticleMd")}
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={!hasAiAnswer || pdfBusy}
                className="flex w-full items-center px-3 py-1.5 text-left text-sm hover:bg-[color-mix(in_srgb,var(--line)_50%,transparent)] disabled:cursor-not-allowed disabled:opacity-40"
                onClick={() => void runExport("both-pdf")}
              >
                {t("summaryArticlePdf")}
              </button>
            </div>
          ) : null}
        </div>
        <div className="relative" ref={typoRef}>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setTypoOpen((v) => !v)}
            title={t("typographyTitle")}
            aria-expanded={typoOpen}
            aria-haspopup="dialog"
          >
            <Type className="h-3.5 w-3.5" />
            {t("type")}
          </Button>
          {typoOpen ? (
            <div
              className="absolute left-0 top-full z-30 mt-1 w-[17.5rem] rounded-lg border border-[var(--line)] bg-[var(--bg-elevated)] p-3 shadow-[var(--shadow)]"
              role="dialog"
              aria-label="Okuma tipografisi"
            >
              <p className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-[var(--muted)]">
                Boyut
              </p>
              <div className="mb-3 flex items-center gap-2">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={!typography.canSmaller}
                  onClick={() => typography.bumpSize(-1)}
                  title={t("smaller")}
                  aria-label={t("smallerAria")}
                >
                  <Minus className="h-3.5 w-3.5" />
                  <span className="text-[11px]">A</span>
                </Button>
                <input
                  type="range"
                  min={0}
                  max={FONT_SIZES.length - 1}
                  step={1}
                  value={FONT_SIZES.indexOf(typography.fontSize)}
                  onChange={(e) => {
                    const next = FONT_SIZES[Number(e.target.value)] as ReaderFontSize;
                    typography.setFontSize(next);
                  }}
                  className="h-1.5 flex-1 accent-[var(--accent)]"
                  aria-label={t("textSize")}
                />
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={!typography.canLarger}
                  onClick={() => typography.bumpSize(1)}
                  title={t("larger")}
                  aria-label={t("largerAria")}
                >
                  <Plus className="h-3.5 w-3.5" />
                  <span className="text-sm">A</span>
                </Button>
                <span className="w-8 text-right text-xs tabular-nums text-[var(--muted)]">
                  {typography.fontSize}
                </span>
              </div>
              <p className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-[var(--muted)]">
                {t("fontFamily")}
              </p>
              <div className="flex flex-col gap-1">
                {READER_FONTS.map((f) => {
                  const active = typography.fontId === f.id;
                  return (
                    <button
                      key={f.id}
                      type="button"
                      onClick={() => typography.setFontId(f.id as ReaderFontId)}
                      className={`rounded-md px-2.5 py-1.5 text-left text-sm transition-colors ${
                        active
                          ? "bg-[color-mix(in_srgb,var(--accent)_18%,transparent)] text-[var(--text)] ring-1 ring-[var(--accent)]"
                          : "hover:bg-[color-mix(in_srgb,var(--line)_50%,transparent)]"
                      }`}
                      style={{ fontFamily: f.stack }}
                    >
                      {f.label}
                      <span className="ml-2 text-[11px] text-[var(--muted)]" style={{ fontFamily: "inherit" }}>
                        Aa
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>
          ) : null}
        </div>
      </div>
      {dlStatus ? (
        <p className="border-b border-[var(--line)] px-4 py-1 text-[11px] text-[var(--muted)]" role="status">
          {dlStatus}
        </p>
      ) : null}
      <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5" style={readerStyle}>
        <p className="text-xs font-medium uppercase tracking-wide text-[var(--muted)]">
          {article.feed_title}
          {article.published ? ` · ${fullDate(article.published)}` : ""}
        </p>
        <h1 className="article-title mt-1 text-[1.85rem] font-medium leading-tight">
          {article.url ? (
            <a
              href={article.url}
              className="hover:underline"
              onClick={(e) => {
                e.preventDefault();
                if (article.url) void openExternal(article.url);
              }}
            >
              {article.title}
            </a>
          ) : (
            article.title
          )}
        </h1>
        {fullBusy ? (
          <div className="mt-4 rounded-lg border border-[var(--line)] bg-[var(--bg-elevated)] px-4 py-3 text-sm text-[var(--muted)]">
            {t("fullTextLoading")}
          </div>
        ) : null}
        {fullError ? (
          <div className="mt-4 whitespace-pre-wrap rounded-lg border border-[var(--danger)]/40 bg-[color-mix(in_srgb,var(--danger)_8%,transparent)] px-4 py-3 text-sm">
            {fullError}
          </div>
        ) : null}
        <AiAnswerBlock aiBusy={aiBusy} ai={ai} aiError={aiError} bodyRef={aiBodyRef} onFeedSubscribed={onFeedSubscribed} />
        <div
          ref={articleBodyRef}
          className="article-body mt-6"
          onClick={(e) => {
            const a = (e.target as HTMLElement).closest("a");
            if (a?.href) {
              e.preventDefault();
              void openExternal(a.href);
            }
          }}
          dangerouslySetInnerHTML={{ __html: article.content_html || `<p>${article.summary || "No content provided by this feed."}</p>` }}
        />
      </div>
    </div>
  );
}
