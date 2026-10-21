import { t } from "@/lib/i18n";
import { useState } from "react";
import { Sparkles, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import type { AskAiContext, AskAiResponse, Selection } from "@/lib/types";
import { ResizeHandle, usePersistedSize } from "@/components/ui/resizable";
import { stripToolArtifacts } from "@/lib/aiMarkdown";

function selectionContext(
  selection: Selection,
  articleId: number | null,
  query: string,
): AskAiContext {
  const base: AskAiContext = {
    article_id: articleId,
    query: query.trim() || null,
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

const PRESETS = [
  "Write a short report on the current selection and save it to a new notebook note via write_note.",
  "Summarize the open article and append the bullets to a notebook note titled Article notes.",
  "Suggest 5 RSS feeds related to my current reading, as a markdown list (suggest_feeds).",
  "List my notebook notes.",
];

export function AskAiPanel({
  open,
  onClose,
  selection,
  articleId,
  query,
  onDone,
}: {
  open: boolean;
  onClose: () => void;
  selection: Selection;
  articleId: number | null;
  query: string;
  onDone?: () => void;
}) {
  const [prompt, setPrompt] = useState(PRESETS[0]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<AskAiResponse | null>(null);
  const [panelH, setPanelH] = usePersistedSize("feedcairn.pane.askAi", 320, 180, 700);

  async function run() {
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const res = await api.askAi({
        prompt,
        context: selectionContext(selection, articleId, query),
      });
      setResult(res);
      onDone?.();
    } catch (e) {
      const err = e as { hint?: string; error?: string };
      let msg = err?.hint || err?.error || (e instanceof Error ? e.message : String(e));
      if (typeof msg === "string") {
        const t = msg.trim();
        if (t.startsWith("{") && t.includes('"error"')) {
          try {
            const p = JSON.parse(t) as { hint?: string; error?: string };
            if (p?.hint) msg = p.hint;
            else if (typeof p?.error === "string" && p.error.trim()) msg = p.error;
          } catch {
            /* keep */
          }
        }
      }
      setError(String(msg));
    } finally {
      setBusy(false);
    }
  }

  if (!open) return null;

  return (
    <div className="fixed inset-x-0 bottom-0 z-40 mx-auto max-w-3xl p-3 md:left-[250px]">
      <div
        className="flex flex-col overflow-hidden rounded-xl border border-[var(--line)] bg-[var(--bg-elevated)] shadow-[var(--shadow)]"
        style={{ height: panelH }}
      >
        <ResizeHandle
          direction="ns"
          value={panelH}
          onChange={setPanelH}
          min={180}
          max={700}
          invert
          className="w-full"
          title="Resize Ask AI panel"
        />
        <div className="flex shrink-0 items-center gap-2 border-b border-[var(--line)] px-3 py-2">
          <Sparkles className="h-4 w-4 text-[var(--accent)]" />
          <div className="text-sm font-semibold">Ask AI</div>
          <div className="text-[11px] text-[var(--muted)]">allowlisted tools only · fail closed</div>
          <div className="flex-1" />
          <Button variant="ghost" size="icon" onClick={onClose}>
            <X className="h-4 w-4" />
          </Button>
        </div>
        <div className="min-h-0 flex-1 space-y-2 overflow-y-auto p-3">
          <div className="flex flex-wrap gap-1">
            {PRESETS.map((p) => (
              <button
                key={p.slice(0, 24)}
                type="button"
                className="rounded-full border border-[var(--line)] px-2 py-0.5 text-[11px] text-[var(--muted)] hover:border-[var(--accent)] hover:text-[var(--text)]"
                onClick={() => setPrompt(p)}
              >
                {p.slice(0, 42)}…
              </button>
            ))}
          </div>
          <textarea
            className="h-24 w-full resize-y rounded-md border border-[var(--line)] bg-[var(--bg)] p-2 text-sm outline-none focus:ring-2 focus:ring-[var(--accent)]"
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            placeholder="Ask for a report, summary, feed suggestions, or notebook write…"
          />
          <div className="flex items-center gap-2">
            <Button onClick={() => void run()} disabled={busy || !prompt.trim()}>
              {busy ? "Running CLI…" : "Run"}
            </Button>
            <span className="text-[11px] text-[var(--muted)]">
              Context: {selection.kind}
              {articleId ? ` · article #${articleId}` : ""}
              {query.trim() ? ` · search “${query.trim()}”` : ""}
            </span>
          </div>
          {error ? (
            <pre className="whitespace-pre-wrap rounded-md border border-[var(--danger)]/40 bg-[color-mix(in_srgb,var(--danger)_8%,transparent)] p-2 text-xs">
              {error}
            </pre>
          ) : null}
          {result ? (
            <div className="max-h-64 space-y-2 overflow-y-auto rounded-md border border-[var(--line)] p-2 text-sm">
              <p className="text-[11px] uppercase tracking-wide text-[var(--muted)]">
                {result.provider}
              </p>
              <pre className="whitespace-pre-wrap font-sans text-sm leading-relaxed">
                {stripToolArtifacts(result.output || "").trim()
                  || (result.tool_results || [])
                      .filter((t) => t.ok && (t.detail || "").trim().length > 40)
                      .map((t) => t.detail)
                      .join("\n\n")
                  || result.output
                  || t("emptyAnswer")}
              </pre>
              {result.tool_results?.length ? (
                <div className="space-y-1 border-t border-[var(--line)] pt-2">
                  <div className="text-[11px] font-semibold uppercase text-[var(--muted)]">Tools</div>
                  {result.tool_results.map((t, i) => (
                    <div key={`${t.name}-${i}`} className="rounded bg-[var(--sidebar)] px-2 py-1 text-xs">
                      <span className={t.ok ? "text-[var(--accent)]" : "text-[var(--danger)]"}>
                        {t.ok ? "✓" : "✗"} {t.name}
                      </span>
                      <pre className="mt-1 whitespace-pre-wrap font-sans text-[11px] text-[var(--muted)]">
                        {t.detail}
                      </pre>
                    </div>
                  ))}
                </div>
              ) : null}
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
