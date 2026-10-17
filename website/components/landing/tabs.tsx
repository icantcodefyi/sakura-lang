import type { ReactNode } from "react";
import { Tabs as TabsRoot, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

/** Segmented tabs over shadcn's Tabs (Radix: roving focus, arrow keys, Home/End). */
export function Tabs({
  items,
}: {
  /** `short` replaces `label` on phones, where four equal tabs leave ~80px each. */
  items: { label: string; short?: string; panel: ReactNode }[];
}) {
  return (
    <TabsRoot defaultValue={items[0].label} className="gap-4">
      <TabsList
        aria-label="How an agent sees Sakura Lang"
        className="grid h-11 w-full rounded-lg border border-border bg-muted p-1"
        style={{ gridTemplateColumns: `repeat(${items.length}, minmax(0, 1fr))` }}
      >
        {items.map((it) => (
          <TabsTrigger
            key={it.label}
            value={it.label}
            className="h-full truncate rounded-md text-[13px] text-muted-foreground transition-[color,background-color,box-shadow] duration-200 ease-out data-active:bg-card data-active:text-foreground data-active:shadow-[0_1px_2px_hsl(var(--shadow-color)/0.14),0_0_0_1px_var(--border)] sm:text-[14px] dark:data-active:border-transparent dark:data-active:bg-card"
          >
            {it.short ? (
              <>
                <span className="sm:hidden">{it.short}</span>
                <span className="max-sm:hidden">{it.label}</span>
              </>
            ) : (
              it.label
            )}
          </TabsTrigger>
        ))}
      </TabsList>
      {items.map((it) => (
        <TabsContent
          key={it.label}
          value={it.label}
          className="motion-safe:animate-[note-in_250ms_var(--ease-out)_both]"
        >
          {it.panel}
        </TabsContent>
      ))}
    </TabsRoot>
  );
}
