import * as React from "react";
import { cn } from "@/lib/utils";

export function Input({ className, ...props }: React.ComponentProps<"input">) {
  return (
    <input
      className={cn(
        "flex h-8 w-full rounded-md border border-[var(--line)] bg-[var(--bg-elevated)] px-3 text-sm outline-none placeholder:text-[var(--muted)] focus:ring-2 focus:ring-[var(--accent)]",
        className,
      )}
      {...props}
    />
  );
}
