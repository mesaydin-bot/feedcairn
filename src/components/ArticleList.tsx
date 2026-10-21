import { CheckCheck, Star } from "lucide-react";
import type { ArticleListItem } from "@/lib/types";
import { relativeTime } from "@/lib/time";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";

export function ArticleList({
  items,
  selectedId,
  loading,
  error,
  emptyLabel,
  onSelect,
  onMarkAllRead,
}: {
  items: ArticleListItem[];
  selectedId: number | null;
  loading: boolean;
  error: string | null;
  emptyLabel: string;
  onSelect: (id: number) => void;
  onMarkAllRead: () => void;
}) {
  return (
    <div className="flex h-full min-h-0 flex-col border-r border-[var(--line)] bg-[var(--bg)]">
      <div className="flex items-center justify-between border-b border-[var(--line)] px-3 py-2">
        <span className="text-xs font-medium text-[var(--muted)]">
          {loading ? "Loading…" : `${items.length} article${items.length === 1 ? "" : "s"}`}
        </span>
        <Button variant="ghost" size="sm" onClick={onMarkAllRead} title="Mark all read in this feed, folder, or view">
          <CheckCheck className="h-3.5 w-3.5" />
          Mark all read
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {error ? (
          <p className="px-4 py-6 text-sm text-[var(--danger)]">{error}</p>
        ) : null}
        {!loading && !error && items.length === 0 ? (
          <p className="px-4 py-10 text-center text-sm text-[var(--muted)]">{emptyLabel}</p>
        ) : null}
        {items.map((item) => (
          <button
            key={item.id}
            onClick={() => onSelect(item.id)}
            className={cn(
              "flex w-full flex-col gap-0.5 border-b border-[var(--line)] px-3 py-2.5 text-left hover:bg-[color-mix(in_srgb,var(--line)_35%,transparent)]",
              selectedId === item.id && "bg-[color-mix(in_srgb,var(--accent)_12%,transparent)]",
              item.is_read && "opacity-70",
            )}
          >
            <div className="flex items-start gap-2">
              <span
                className={cn(
                  "mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full",
                  item.is_read ? "bg-transparent" : "bg-[var(--unread)]",
                )}
              />
              <span
                className={cn(
                  "min-w-0 flex-1 text-[13.5px] leading-snug",
                  !item.is_read && "font-semibold",
                )}
              >
                {item.title}
              </span>
              {item.is_saved ? (
                <Star className="mt-0.5 h-3.5 w-3.5 shrink-0 fill-[var(--saved)] text-[var(--saved)]" />
              ) : null}
            </div>
            <div className="flex items-center gap-2 pl-3.5 text-[11px] text-[var(--muted)]">
              <span className="truncate">{item.feed_title}</span>
              <span>·</span>
              <span>{relativeTime(item.published)}</span>
            </div>
            {item.summary ? (
              <p className="line-clamp-2 pl-3.5 text-[12px] leading-snug text-[var(--muted)]">
                {item.summary}
              </p>
            ) : null}
          </button>
        ))}
      </div>
    </div>
  );
}
