import { useState } from "react";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { FolderNode } from "@/lib/types";

function folderOptions(nodes: FolderNode[], prefix = ""): { id: number; name: string }[] {
  return nodes.flatMap((n) => [
    { id: n.folder.id, name: `${prefix}${n.folder.name}` },
    ...folderOptions(n.children, `${prefix}${n.folder.name} / `),
  ]);
}

export function AddFeedDialog({
  open,
  onOpenChange,
  folders,
  onSubmit,
  busy,
  error,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  folders: FolderNode[];
  onSubmit: (url: string, folderId: number | null) => Promise<void>;
  busy: boolean;
  error: string | null;
}) {
  const [url, setUrl] = useState("");
  const [folderId, setFolderId] = useState<string>("");
  const opts = folderOptions(folders);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title="Add feed">
        <form
          className="space-y-3"
          onSubmit={async (e) => {
            e.preventDefault();
            await onSubmit(url.trim(), folderId ? Number(folderId) : null);
            setUrl("");
          }}
        >
          <label className="block text-sm">
            <span className="mb-1 block text-[var(--muted)]">Feed URL</span>
            <Input
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://example.com/feed.xml"
              autoFocus
              required
            />
          </label>
          <label className="block text-sm">
            <span className="mb-1 block text-[var(--muted)]">Folder</span>
            <select
              className="h-8 w-full rounded-md border border-[var(--line)] bg-[var(--bg-elevated)] px-2 text-sm"
              value={folderId}
              onChange={(e) => setFolderId(e.target.value)}
            >
              <option value="">Unfiled</option>
              {opts.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.name}
                </option>
              ))}
            </select>
          </label>
          {error ? <p className="text-sm text-[var(--danger)]">{error}</p> : null}
          <div className="flex justify-end gap-2 pt-2">
            <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={busy || !url.trim()}>
              {busy ? "Fetching…" : "Subscribe"}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function ImportOpmlDialog({
  open,
  onOpenChange,
  onImport,
  onExport,
  busy,
  message,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  onImport: (xml: string) => Promise<void>;
  onExport: () => Promise<void>;
  busy: boolean;
  message: string | null;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title="Import / export OPML">
        <p className="mb-3 text-sm text-[var(--muted)]">
          Choose an OPML export from any RSS reader. Nested outlines become folders.
        </p>
        <input
          type="file"
          accept=".opml,.xml,text/xml"
          disabled={busy}
          onChange={async (e) => {
            const file = e.target.files?.[0];
            if (!file) return;
            const xml = await file.text();
            await onImport(xml);
          }}
        />
        <p className="mt-3 text-xs text-[var(--muted)]">
          Smoke-test file:{" "}
          <a className="text-[var(--accent)] underline" href="/sample.opml" download>
            sample.opml
          </a>
        </p>
        <div className="mt-4">
          <Button type="button" variant="outline" onClick={() => void onExport()}>
            Export library as OPML
          </Button>
        </div>
        {message ? <p className="mt-3 text-sm">{message}</p> : null}
      </DialogContent>
    </Dialog>
  );
}

export function FolderDialog({
  open,
  onOpenChange,
  onSubmit,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  onSubmit: (name: string) => Promise<void>;
}) {
  const [name, setName] = useState("");
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title="New folder">
        <form
          className="space-y-3"
          onSubmit={async (e) => {
            e.preventDefault();
            await onSubmit(name.trim());
            setName("");
          }}
        >
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Folder name"
            autoFocus
          />
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={!name.trim()}>
              Create
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
