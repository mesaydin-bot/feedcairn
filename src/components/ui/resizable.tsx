import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import { cn } from "@/lib/utils";

export function readPersistedNumber(key: string, fallback: number): number {
  try {
    const raw = localStorage.getItem(key);
    if (raw == null) return fallback;
    const n = Number(raw);
    return Number.isFinite(n) ? n : fallback;
  } catch {
    return fallback;
  }
}

export function writePersistedNumber(key: string, value: number) {
  try {
    localStorage.setItem(key, String(Math.round(value)));
  } catch {
    /* ignore quota / private mode */
  }
}

/** Persist a numeric size in localStorage; updates on every change. */
export function usePersistedSize(key: string, defaultValue: number, min: number, max: number) {
  const [size, setSizeState] = useState(() =>
    Math.min(max, Math.max(min, readPersistedNumber(key, defaultValue))),
  );

  const setSize = useCallback(
    (next: number | ((prev: number) => number)) => {
      setSizeState((prev) => {
        const raw = typeof next === "function" ? next(prev) : next;
        const clamped = Math.min(max, Math.max(min, raw));
        writePersistedNumber(key, clamped);
        return clamped;
      });
    },
    [key, min, max],
  );

  return [size, setSize] as const;
}

type ResizeHandleProps = {
  /** `ew` = vertical bar (resize width); `ns` = horizontal bar (resize height). */
  direction: "ew" | "ns";
  value: number;
  onChange: (next: number) => void;
  min?: number;
  max?: number;
  /**
   * When true, moving the pointer in the positive axis direction decreases `value`
   * (use for a left edge of a right-side panel, or top edge of a bottom sheet).
   */
  invert?: boolean;
  className?: string;
  title?: string;
};

/**
 * Drag handle for resizing an adjacent pane. No extra dependencies —
 * pointer capture + localStorage persistence via usePersistedSize.
 */
export function ResizeHandle({
  direction,
  value,
  onChange,
  min = 120,
  max = 800,
  invert = false,
  className,
  title = "Drag to resize",
}: ResizeHandleProps) {
  const dragging = useRef(false);
  const startPtr = useRef(0);
  const startVal = useRef(0);

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    dragging.current = true;
    startPtr.current = direction === "ew" ? e.clientX : e.clientY;
    startVal.current = value;
    e.currentTarget.setPointerCapture(e.pointerId);
    document.body.style.cursor = direction === "ew" ? "col-resize" : "row-resize";
    document.body.style.userSelect = "none";
  };

  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (!dragging.current) return;
    const pos = direction === "ew" ? e.clientX : e.clientY;
    const delta = pos - startPtr.current;
    const next = startVal.current + (invert ? -delta : delta);
    onChange(Math.min(max, Math.max(min, next)));
  };

  const endDrag = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (!dragging.current) return;
    dragging.current = false;
    try {
      e.currentTarget.releasePointerCapture(e.pointerId);
    } catch {
      /* already released */
    }
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
  };

  useEffect(() => {
    return () => {
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
  }, []);

  const isEw = direction === "ew";

  return (
    <div
      role="separator"
      aria-orientation={isEw ? "vertical" : "horizontal"}
      aria-valuenow={Math.round(value)}
      aria-valuemin={min}
      aria-valuemax={max}
      title={title}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onKeyDown={(e) => {
        const step = e.shiftKey ? 24 : 8;
        if (isEw) {
          if (e.key === "ArrowLeft") {
            e.preventDefault();
            onChange(Math.min(max, Math.max(min, value + (invert ? step : -step))));
          } else if (e.key === "ArrowRight") {
            e.preventDefault();
            onChange(Math.min(max, Math.max(min, value + (invert ? -step : step))));
          }
        } else {
          if (e.key === "ArrowUp") {
            e.preventDefault();
            onChange(Math.min(max, Math.max(min, value + (invert ? step : -step))));
          } else if (e.key === "ArrowDown") {
            e.preventDefault();
            onChange(Math.min(max, Math.max(min, value + (invert ? -step : step))));
          }
        }
      }}
      className={cn(
        "group relative z-10 shrink-0 touch-none",
        "focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]",
        isEw
          ? "w-1.5 cursor-col-resize hover:bg-[color-mix(in_srgb,var(--accent)_35%,transparent)] active:bg-[color-mix(in_srgb,var(--accent)_55%,transparent)]"
          : "h-1.5 cursor-row-resize hover:bg-[color-mix(in_srgb,var(--accent)_35%,transparent)] active:bg-[color-mix(in_srgb,var(--accent)_55%,transparent)]",
        className,
      )}
    >
      <div
        className={cn(
          "pointer-events-none absolute rounded-full bg-[var(--line)] opacity-0 transition-opacity group-hover:opacity-100 group-active:opacity-100 group-focus-visible:opacity-100",
          isEw
            ? "left-1/2 top-1/2 h-8 w-0.5 -translate-x-1/2 -translate-y-1/2"
            : "left-1/2 top-1/2 h-0.5 w-8 -translate-x-1/2 -translate-y-1/2",
        )}
      />
    </div>
  );
}

/** Horizontal multi-pane row: children interleaved with optional handles by parent. */
export function ResizableSplit({
  className,
  children,
}: {
  className?: string;
  children: ReactNode;
}) {
  return <div className={cn("flex min-h-0 min-w-0", className)}>{children}</div>;
}
