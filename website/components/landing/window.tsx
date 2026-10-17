import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/** The frame of the landing page's tool windows: the editor, the review, the handler diagram. */
export const WINDOW =
  "overflow-hidden rounded-lg border border-ink bg-card shadow-[0_28px_70px_-40px_hsl(var(--shadow-color)/0.5)] dark:border-strong";

/**
 * A window's ink title bar, in mono. It carries `.dark`, so everything inside reads the dark
 * palette (status colours stay legible on ink) whatever the page theme is.
 */
export function WindowBar({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div
      className={cn(
        "dark flex min-h-8 items-center gap-2.5 border-b border-border bg-code px-3 font-mono text-[11.5px] text-foreground",
        className,
      )}
    >
      {children}
    </div>
  );
}
