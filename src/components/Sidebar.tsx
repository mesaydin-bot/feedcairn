import { t } from "@/lib/i18n";
import { ChevronDown, ChevronRight, Folder as FolderIcon, Rss } from "lucide-react";
import { useEffect, useState, type DragEvent, type MouseEvent, type ReactNode } from "react";
import type { Feed, FolderNode, Selection } from "@/lib/types";
import { cn } from "@/lib/utils";

function Count({ n }: { n: number }) {
  if (!n) return null;
  return (
    <span className="ml-auto rounded-full bg-[color-mix(in_srgb,var(--accent)_18%,transparent)] px-1.5 text-[11px] font-semibold text-[var(--accent)]">
      {n}
    </span>
  );
}

type CtxMenu =
  | {
      kind: "feed";
      feed: Feed;
      x: number;
      y: number;
    }
  | {
      kind: "folder";
      folderId: number;
      folderName: string;
      x: number;
      y: number;
    };

function flattenFolders(nodes: FolderNode[]): { id: number; name: string }[] {
  return nodes.flatMap((n) => [
    { id: n.folder.id, name: n.folder.name },
    ...flattenFolders(n.children),
  ]);
}

function Row({
  active,
  onClick,
  onContextMenu,
  children,
  indent = 0,
  draggable,
  onDragStart,
  onDragOver,
  onDrop,
  onDragLeave,
  dropActive,
}: {
  active: boolean;
  onClick: () => void;
  onContextMenu?: (e: MouseEvent) => void;
  children: ReactNode;
  indent?: number;
  draggable?: boolean;
  onDragStart?: (e: DragEvent) => void;
  onDragOver?: (e: DragEvent) => void;
  onDrop?: (e: DragEvent) => void;
  onDragLeave?: (e: DragEvent) => void;
  dropActive?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      onContextMenu={onContextMenu}
      draggable={draggable}
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDrop={onDrop}
      onDragLeave={onDragLeave}
      style={{ paddingLeft: 8 + indent * 14 }}
      className={cn(
        "flex w-full items-center gap-2 rounded-md py-1 pr-2 text-left text-[13px] hover:bg-[color-mix(in_srgb,var(--line)_55%,transparent)]",
        active && "bg-[color-mix(in_srgb,var(--accent)_16%,transparent)] font-medium",
        dropActive && "ring-1 ring-[var(--accent)] bg-[color-mix(in_srgb,var(--accent)_12%,transparent)]",
      )}
    >
      {children}
    </button>
  );
}

function FeedRow({
  feed,
  selection,
  onSelect,
  indent,
  onContextMenu,
  onDragFeed,
}: {
  feed: Feed;
  selection: Selection;
  onSelect: (s: Selection) => void;
  indent: number;
  onContextMenu: (e: MouseEvent, feed: Feed) => void;
  onDragFeed: (feedId: number, e: DragEvent) => void;
}) {
  return (
    <Row
      active={selection.kind === "feed" && selection.id === feed.id}
      onClick={() => onSelect({ kind: "feed", id: feed.id, name: feed.title })}
      onContextMenu={(e) => onContextMenu(e, feed)}
      indent={indent}
      draggable
      onDragStart={(e) => onDragFeed(feed.id, e)}
    >
      <Rss className="h-3.5 w-3.5 shrink-0 text-[var(--muted)]" />
      <span className="min-w-0 flex-1 truncate" title={feed.last_error || feed.title}>
        {feed.title}
      </span>
      {feed.last_error ? (
        <span className="text-[10px] text-[var(--danger)]">!</span>
      ) : null}
      <Count n={feed.unread} />
    </Row>
  );
}

function FolderBlock({
  node,
  selection,
  onSelect,
  indent = 0,
  onFeedContext,
  onFolderContext,
  onDragFeed,
  onDropFeed,
  dropTarget,
  setDropTarget,
}: {
  node: FolderNode;
  selection: Selection;
  onSelect: (s: Selection) => void;
  indent?: number;
  onFeedContext: (e: MouseEvent, feed: Feed) => void;
  onFolderContext: (e: MouseEvent, folderId: number, folderName: string) => void;
  onDragFeed: (feedId: number, e: DragEvent) => void;
  onDropFeed: (folderId: number | null) => void;
  dropTarget: number | "root" | null;
  setDropTarget: (t: number | "root" | null) => void;
}) {
  const [open, setOpen] = useState(true);
  const Icon = open ? ChevronDown : ChevronRight;
  const fid = node.folder.id;
  return (
    <div>
      <Row
        active={selection.kind === "folder" && selection.id === fid}
        onClick={() => onSelect({ kind: "folder", id: fid, name: node.folder.name })}
        onContextMenu={(e) => onFolderContext(e, fid, node.folder.name)}
        indent={indent}
        dropActive={dropTarget === fid}
        onDragOver={(e) => {
          e.preventDefault();
          e.dataTransfer.dropEffect = "move";
          setDropTarget(fid);
        }}
        onDragLeave={() => {
          setDropTarget(null);
        }}
        onDrop={(e) => {
          e.preventDefault();
          setDropTarget(null);
          onDropFeed(fid);
        }}
      >
        <span
          role="button"
          tabIndex={0}
          className="shrink-0 rounded p-0.5 hover:bg-[var(--line)]"
          onClick={(e) => {
            e.stopPropagation();
            setOpen((v) => !v);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              e.stopPropagation();
              setOpen((v) => !v);
            }
          }}
        >
          <Icon className="h-3.5 w-3.5" />
        </span>
        <FolderIcon className="h-3.5 w-3.5 shrink-0 text-[var(--accent)]" />
        <span className="min-w-0 flex-1 truncate">{node.folder.name}</span>
        <Count n={node.folder.unread} />
      </Row>
      {open ? (
        <div>
          {node.children.map((c) => (
            <FolderBlock
              key={c.folder.id}
              node={c}
              selection={selection}
              onSelect={onSelect}
              indent={indent + 1}
              onFeedContext={onFeedContext}
              onFolderContext={onFolderContext}
              onDragFeed={onDragFeed}
              onDropFeed={onDropFeed}
              dropTarget={dropTarget}
              setDropTarget={setDropTarget}
            />
          ))}
          {node.feeds.map((f) => (
            <FeedRow
              key={f.id}
              feed={f}
              selection={selection}
              onSelect={onSelect}
              indent={indent + 1}
              onContextMenu={onFeedContext}
              onDragFeed={onDragFeed}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

export function Sidebar({
  folders,
  rootFeeds,
  counts,
  selection,
  onSelect,
  onMoveFeed,
  onRenameFeed,
  onDeleteFeed,
  onRenameFolder,
  onDeleteFolder,
}: {
  folders: FolderNode[];
  rootFeeds: Feed[];
  counts: { unread: number; saved: number; all: number };
  selection: Selection;
  onSelect: (s: Selection) => void;
  onMoveFeed: (feedId: number, folderId: number | null) => void | Promise<void>;
  onRenameFeed: (feedId: number, currentName: string) => void | Promise<void>;
  onDeleteFeed: (feedId: number, name: string) => void | Promise<void>;
  onRenameFolder: (folderId: number, currentName: string) => void | Promise<void>;
  onDeleteFolder: (folderId: number, name: string) => void | Promise<void>;
}) {
  const [menu, setMenu] = useState<CtxMenu | null>(null);
  const [dropTarget, setDropTarget] = useState<number | "root" | null>(null);
  const [draggingFeedId, setDraggingFeedId] = useState<number | null>(null);
  const folderList = flattenFolders(folders);

  useEffect(() => {
    if (!menu) return;
    function close() {
      setMenu(null);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") close();
    }
    window.addEventListener("click", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [menu]);

  function onDragFeed(feedId: number, e: React.DragEvent) {
    setDraggingFeedId(feedId);
    e.dataTransfer.setData("text/feedcairn-feed-id", String(feedId));
    e.dataTransfer.effectAllowed = "move";
  }

  function onDropFeed(folderId: number | null) {
    if (draggingFeedId == null) return;
    void onMoveFeed(draggingFeedId, folderId);
    setDraggingFeedId(null);
    setDropTarget(null);
  }

  return (
    <nav className="relative flex h-full flex-col overflow-y-auto px-2 py-3 text-[var(--text)]">
      <div className="mb-3 space-y-0.5">
        <Row active={selection.kind === "unread"} onClick={() => onSelect({ kind: "unread" })}>
          <span className="flex-1">Unread</span>
          <Count n={counts.unread} />
        </Row>
        <Row active={selection.kind === "all"} onClick={() => onSelect({ kind: "all" })}>
          <span className="flex-1">All articles</span>
          <Count n={counts.all} />
        </Row>
        <Row active={selection.kind === "saved"} onClick={() => onSelect({ kind: "saved" })}>
          <span className="flex-1">Starred / Read later</span>
          <Count n={counts.saved} />
        </Row>
      </div>
      <div className="px-2 pb-1 text-[11px] font-semibold uppercase tracking-wide text-[var(--muted)]">
        Collections
      </div>
      {folders.length === 0 && rootFeeds.length === 0 ? (
        <p className="px-2 py-3 text-xs leading-relaxed text-[var(--muted)]">
          No feeds yet. Import an OPML file or add a feed URL to start reading.
        </p>
      ) : null}
      {folders.map((n) => (
        <FolderBlock
          key={n.folder.id}
          node={n}
          selection={selection}
          onSelect={onSelect}
          onFeedContext={(e, feed) => {
            e.preventDefault();
            e.stopPropagation();
            setMenu({ kind: "feed", feed, x: e.clientX, y: e.clientY });
          }}
          onFolderContext={(e, folderId, folderName) => {
            e.preventDefault();
            e.stopPropagation();
            setMenu({ kind: "folder", folderId, folderName, x: e.clientX, y: e.clientY });
          }}
          onDragFeed={onDragFeed}
          onDropFeed={onDropFeed}
          dropTarget={dropTarget}
          setDropTarget={setDropTarget}
        />
      ))}
      <div
        className={cn(
          "mt-1 rounded-md px-1 py-1",
          dropTarget === "root" && "ring-1 ring-[var(--accent)] bg-[color-mix(in_srgb,var(--accent)_10%,transparent)]",
        )}
        onDragOver={(e) => {
          e.preventDefault();
          e.dataTransfer.dropEffect = "move";
          setDropTarget("root");
        }}
        onDragLeave={() => setDropTarget((t) => (t === "root" ? null : t))}
        onDrop={(e) => {
          e.preventDefault();
          onDropFeed(null);
        }}
      >
        {rootFeeds.length > 0 ? (
          <div className="px-2 pb-0.5 text-[10px] uppercase tracking-wide text-[var(--muted)]">
            {t("unfiled")}
          </div>
        ) : draggingFeedId != null ? (
          <div className="px-2 py-2 text-[11px] text-[var(--muted)]">{t("dropUnfiled")}</div>
        ) : null}
        {rootFeeds.map((f) => (
          <FeedRow
            key={f.id}
            feed={f}
            selection={selection}
            onSelect={onSelect}
            indent={0}
            onContextMenu={(e, feed) => {
              e.preventDefault();
              e.stopPropagation();
              setMenu({ kind: "feed", feed, x: e.clientX, y: e.clientY });
            }}
            onDragFeed={onDragFeed}
          />
        ))}
      </div>

      {menu ? (
        <div
          className="fixed z-[100] min-w-[11rem] rounded-md border border-[var(--line)] bg-[var(--bg-elevated)] py-1 text-sm shadow-[var(--shadow)]"
          style={{ left: menu.x, top: menu.y }}
          onClick={(e) => e.stopPropagation()}
          onContextMenu={(e) => e.preventDefault()}
        >
          {menu.kind === "feed" ? (
            <>
              <div className="px-3 py-1 text-[11px] font-semibold uppercase text-[var(--muted)]">
                {t("move")}
              </div>
              <button
                type="button"
                className="block w-full px-3 py-1.5 text-left hover:bg-[color-mix(in_srgb,var(--line)_55%,transparent)]"
                onClick={() => {
                  void onMoveFeed(menu.feed.id, null);
                  setMenu(null);
                }}
              >
                {t("unfiled")}
              </button>
              {folderList.map((f) => (
                <button
                  key={f.id}
                  type="button"
                  className="block w-full px-3 py-1.5 text-left hover:bg-[color-mix(in_srgb,var(--line)_55%,transparent)]"
                  onClick={() => {
                    void onMoveFeed(menu.feed.id, f.id);
                    setMenu(null);
                  }}
                >
                  {f.name}
                </button>
              ))}
              <div className="my-1 border-t border-[var(--line)]" />
              <button
                type="button"
                className="block w-full px-3 py-1.5 text-left hover:bg-[color-mix(in_srgb,var(--line)_55%,transparent)]"
                onClick={() => {
                  void onRenameFeed(menu.feed.id, menu.feed.title);
                  setMenu(null);
                }}
              >
                {t("rename")}
              </button>
              <button
                type="button"
                className="block w-full px-3 py-1.5 text-left text-[var(--danger)] hover:bg-[color-mix(in_srgb,var(--danger)_12%,transparent)]"
                onClick={() => {
                  void onDeleteFeed(menu.feed.id, menu.feed.title);
                  setMenu(null);
                }}
              >
                {t("deleteAction")}
              </button>
            </>
          ) : (
            <>
              <button
                type="button"
                className="block w-full px-3 py-1.5 text-left hover:bg-[color-mix(in_srgb,var(--line)_55%,transparent)]"
                onClick={() => {
                  void onRenameFolder(menu.folderId, menu.folderName);
                  setMenu(null);
                }}
              >
                {t("rename")}
              </button>
              <button
                type="button"
                className="block w-full px-3 py-1.5 text-left text-[var(--danger)] hover:bg-[color-mix(in_srgb,var(--danger)_12%,transparent)]"
                onClick={() => {
                  void onDeleteFolder(menu.folderId, menu.folderName);
                  setMenu(null);
                }}
              >
                {t("deleteAction")}
              </button>
            </>
          )}
        </div>
      ) : null}
    </nav>
  );
}
