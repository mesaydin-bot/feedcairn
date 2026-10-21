import { t as tr } from "@/lib/i18n";
import { useCallback, useEffect, useRef, useState } from "react";
import { Bookmark, Pencil, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import type { PromptTemplate } from "@/lib/types";

function withSlash(body: string): string {
  const t = body.trim();
  if (!t) return "/";
  return t.startsWith("/") ? t : `/${t}`;
}

export function PromptTemplatesControls({
  query,
  aiMode,
  onFill,
  onRun: _onRun,
}: {
  query: string;
  aiMode: boolean;
  onFill: (slashPrompt: string) => void;
  onRun?: (slashPrompt: string) => void;
}) {
  void _onRun;
  const [open, setOpen] = useState(false);
  const [items, setItems] = useState<PromptTemplate[]>([]);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  const reload = useCallback(async () => {
    try {
      const list = await api.listPromptTemplates();
      setItems(list);
      setErr(null);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    if (open) void reload();
  }, [open, reload]);

  useEffect(() => {
    if (!open) return;
    function onDoc(e: MouseEvent) {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [open]);

  async function saveCurrent() {
    if (!aiMode || !query.trim() || query.trim() === "/") return;
    const name = window.prompt(tr("templateName"), query.slice(1).trim().slice(0, 40) || tr("template"));
    if (!name?.trim()) return;
    setBusy(true);
    try {
      await api.createPromptTemplate(name.trim(), query);
      await reload();
      setOpen(true);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function rename(t: PromptTemplate) {
    const name = window.prompt("Yeni ad:", t.name);
    if (!name?.trim() || name.trim() === t.name) return;
    setBusy(true);
    try {
      await api.updatePromptTemplate(t.id, { name: name.trim() });
      await reload();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function remove(t: PromptTemplate) {
    if (!window.confirm(tr("deleteTemplateConfirm", { name: t.name }))) return;
    setBusy(true);
    try {
      await api.deletePromptTemplate(t.id);
      await reload();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  function pick(t: PromptTemplate) {
    const filled = withSlash(t.body);
    onFill(filled);
    setOpen(false);
    // Enter in search bar runs; optional auto-run not forced.
  }

  const canSave = aiMode && query.trim().length > 1;

  return (
    <div ref={rootRef} className="relative flex shrink-0 items-center gap-1">
      {canSave ? (
        <Button
          variant="ghost"
          size="sm"
          disabled={busy}
          onClick={() => void saveCurrent()}
          title={tr("saveTemplateTitle")}
        >
          {tr("saveTemplate")}
        </Button>
      ) : null}
      <Button
        variant={open ? "default" : "ghost"}
        size="icon"
        onClick={() => setOpen((v) => !v)}
        title={tr("templates")}
      >
        <Bookmark className="h-4 w-4" />
      </Button>
      {open ? (
        <div className="absolute right-0 top-full z-50 mt-1 w-[22rem] max-w-[90vw] rounded-lg border border-[var(--line)] bg-[var(--bg-elevated)] p-2 shadow-[var(--shadow)]">
          <div className="mb-2 flex items-center justify-between px-1">
            <span className="text-xs font-semibold uppercase tracking-wide text-[var(--muted)]">
              {tr("templates")}
            </span>
            <div className="flex items-center gap-1">
              {canSave ? (
                <Button variant="outline" size="sm" disabled={busy} onClick={() => void saveCurrent()}>
                  {tr("save")}
                </Button>
              ) : null}
              <Button variant="ghost" size="icon" onClick={() => setOpen(false)} title={tr("close")}>
                <X className="h-3.5 w-3.5" />
              </Button>
            </div>
          </div>
          {err ? <p className="mb-2 px-1 text-xs text-[var(--danger)]">{err}</p> : null}
          {items.length === 0 ? (
            <p className="px-2 py-3 text-xs text-[var(--muted)]">
              {tr("noTemplates")}
            </p>
          ) : (
            <ul className="max-h-64 space-y-0.5 overflow-y-auto">
              {items.map((t) => (
                <li
                  key={t.id}
                  className="group flex items-start gap-1 rounded-md px-2 py-1.5 hover:bg-[color-mix(in_srgb,var(--line)_55%,transparent)]"
                >
                  <button
                    type="button"
                    className="min-w-0 flex-1 text-left"
                    onClick={() => pick(t)}
                    title={`/${t.body}`}
                  >
                    <div className="truncate text-sm font-medium">{t.name}</div>
                    <div className="truncate text-[11px] text-[var(--muted)]">/{t.body}</div>
                  </button>
                  <button
                    type="button"
                    className="shrink-0 rounded p-1 opacity-60 hover:bg-[var(--line)] hover:opacity-100"
                    title={tr("rename")}
                    onClick={() => void rename(t)}
                  >
                    <Pencil className="h-3.5 w-3.5" />
                  </button>
                  <button
                    type="button"
                    className="shrink-0 rounded p-1 opacity-60 hover:bg-[color-mix(in_srgb,var(--danger)_12%,transparent)] hover:opacity-100"
                    title={tr("deleteAction")}
                    onClick={() => void remove(t)}
                  >
                    <Trash2 className="h-3.5 w-3.5 text-[var(--danger)]" />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      ) : null}
    </div>
  );
}
