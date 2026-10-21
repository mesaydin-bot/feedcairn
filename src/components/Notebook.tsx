import { useCallback, useEffect, useState } from "react";
import { Download, Plus, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import type { Note } from "@/lib/types";
import { cn } from "@/lib/utils";
import { ResizeHandle, usePersistedSize } from "@/components/ui/resizable";

function downloadMd(note: Note) {
  const body = `# ${note.title}\n\n${note.body_md || ""}\n`;
  const blob = new Blob([body], { type: "text/markdown;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  const safe = note.title.replace(/[^\w\-]+/g, "_").slice(0, 60) || "note";
  a.download = `${safe}.md`;
  a.click();
  URL.revokeObjectURL(url);
}

export function NotebookPanel({
  open,
  onClose,
  refreshToken = 0,
}: {
  open: boolean;
  onClose: () => void;
  refreshToken?: number;
}) {
  const [notes, setNotes] = useState<Note[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);
  const [panelW, setPanelW] = usePersistedSize("feedcairn.pane.notebook", 420, 280, 720);
  const [listW, setListW] = usePersistedSize("feedcairn.pane.notebookList", 150, 100, 320);

  const selected = notes.find((n) => n.id === selectedId) || null;

  const reload = useCallback(async () => {
    const rows = await api.listNotes();
    setNotes(rows);
    return rows;
  }, []);

  useEffect(() => {
    if (!open) return;
    void reload()
      .then((rows) => {
        if (!rows.length) {
          setSelectedId(null);
          setTitle("");
          setBody("");
          setDirty(false);
          return;
        }
        const keep = selectedId != null && rows.some((r) => r.id === selectedId);
        const pick = keep ? rows.find((r) => r.id === selectedId)! : rows[0];
        setSelectedId(pick.id);
        if (!dirty) {
          setTitle(pick.title);
          setBody(pick.body_md);
        }
      })
      .catch((e) => setError(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, reload, refreshToken]);

  useEffect(() => {
    if (!selectedId) return;
    const n = notes.find((x) => x.id === selectedId);
    if (n && !dirty) {
      setTitle(n.title);
      setBody(n.body_md);
    }
  }, [selectedId, notes, dirty]);

  async function select(id: number) {
    if (dirty && selectedId != null) {
      await save(selectedId, title, body);
    }
    const n = notes.find((x) => x.id === id);
    setSelectedId(id);
    if (n) {
      setTitle(n.title);
      setBody(n.body_md);
      setDirty(false);
    }
  }

  async function save(id: number, t: string, b: string) {
    setBusy(true);
    setError(null);
    try {
      const updated = await api.updateNote(id, { title: t, body_md: b });
      setNotes((rows) => rows.map((r) => (r.id === id ? updated : r)));
      setDirty(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function create() {
    setBusy(true);
    setError(null);
    try {
      if (dirty && selectedId != null) await save(selectedId, title, body);
      const n = await api.createNote("Untitled", "");
      const rows = await reload();
      setSelectedId(n.id);
      setTitle(n.title);
      setBody(n.body_md);
      setDirty(false);
      setNotes(rows);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (!selectedId) return;
    if (!confirm("Delete this note?")) return;
    setBusy(true);
    try {
      await api.deleteNote(selectedId);
      const rows = await reload();
      const next = rows[0] || null;
      setSelectedId(next?.id ?? null);
      setTitle(next?.title ?? "");
      setBody(next?.body_md ?? "");
      setDirty(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  if (!open) return null;

  return (
    <>
    <ResizeHandle
      direction="ew"
      value={panelW}
      onChange={setPanelW}
      min={280}
      max={720}
      invert
      className="hidden md:block"
      title="Resize notebook"
    />
    <aside
      className="flex w-full shrink-0 flex-col border-l border-[var(--line)] bg-[var(--sidebar)] max-md:!w-full md:max-w-none"
      style={{ width: panelW }}
    >
      <div className="flex items-center gap-1 border-b border-[var(--line)] px-2 py-1.5">
        <div className="px-1 text-sm font-semibold">Notebook</div>
        <Button variant="ghost" size="icon" onClick={() => void create()} title="New note" disabled={busy}>
          <Plus className="h-4 w-4" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          disabled={!selected || busy}
          onClick={() => selected && downloadMd({ ...selected, title, body_md: body })}
          title="Download .md"
        >
          <Download className="h-4 w-4" />
        </Button>
        <Button variant="ghost" size="icon" disabled={!selectedId || busy} onClick={() => void remove()} title="Delete">
          <Trash2 className="h-4 w-4" />
        </Button>
        <div className="flex-1" />
        <Button variant="ghost" size="icon" onClick={onClose} title="Close notebook">
          <X className="h-4 w-4" />
        </Button>
      </div>
      {error ? <div className="px-3 py-1 text-xs text-[var(--danger)]">{error}</div> : null}
      <div className="flex min-h-0 flex-1">
        <div className="shrink-0 overflow-y-auto border-r border-[var(--line)]" style={{ width: listW }}>
          {notes.length === 0 ? (
            <p className="p-3 text-xs text-[var(--muted)]">No notes yet. Create one or ask AI to write_note.</p>
          ) : (
            notes.map((n) => (
              <button
                key={n.id}
                onClick={() => void select(n.id)}
                className={cn(
                  "block w-full border-b border-[var(--line)] px-2 py-2 text-left text-[13px] hover:bg-[color-mix(in_srgb,var(--line)_50%,transparent)]",
                  n.id === selectedId && "bg-[color-mix(in_srgb,var(--accent)_14%,transparent)]",
                )}
              >
                <div className="truncate font-medium">{n.title || "Untitled"}</div>
                <div className="truncate text-[11px] text-[var(--muted)]">
                  {new Date(n.updated_at * 1000).toLocaleString()}
                </div>
              </button>
            ))
          )}
        </div>
        <ResizeHandle
          direction="ew"
          value={listW}
          onChange={setListW}
          min={100}
          max={320}
          title="Resize note list"
        />
        <div className="flex min-w-0 flex-1 flex-col gap-2 p-2">
          {selectedId ? (
            <>
              <Input
                value={title}
                onChange={(e) => {
                  setTitle(e.target.value);
                  setDirty(true);
                }}
                onBlur={() => {
                  if (dirty) void save(selectedId, title, body);
                }}
                placeholder="Title"
              />
              <textarea
                className="min-h-0 flex-1 resize-none rounded-md border border-[var(--line)] bg-[var(--bg-elevated)] p-2 font-mono text-[13px] leading-relaxed outline-none focus:ring-2 focus:ring-[var(--accent)]"
                value={body}
                onChange={(e) => {
                  setBody(e.target.value);
                  setDirty(true);
                }}
                onBlur={() => {
                  if (dirty) void save(selectedId, title, body);
                }}
                placeholder="Markdown body…"
              />
              <div className="flex items-center justify-between text-[11px] text-[var(--muted)]">
                <span>{dirty ? "Unsaved…" : busy ? "Saving…" : "Saved"}</span>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={!dirty || busy}
                  onClick={() => void save(selectedId, title, body)}
                >
                  Save
                </Button>
              </div>
            </>
          ) : (
            <p className="p-2 text-sm text-[var(--muted)]">Select or create a note.</p>
          )}
        </div>
      </div>
    </aside>
    </>
  );
}
