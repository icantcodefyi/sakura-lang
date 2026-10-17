"use client";

import { useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";

/** Segmented tabs with a sliding indicator (transform-only), arrow-key navigation. */
export function Tabs({
  items,
}: {
  /** `short` replaces `label` on phones, where four equal tabs leave ~80px each. */
  items: { label: string; short?: string; panel: ReactNode }[];
}) {
  const [active, setActive] = useState(0);
  const id = useId();
  const refs = useRef<(HTMLButtonElement | null)[]>([]);

  const onKey = (e: KeyboardEvent) => {
    const last = items.length - 1;
    const next =
      e.key === "ArrowRight"
        ? (active + 1) % items.length
        : e.key === "ArrowLeft"
          ? (active + last) % items.length
          : e.key === "Home"
            ? 0
            : e.key === "End"
              ? last
              : -1;
    if (next < 0) return;
    e.preventDefault();
    setActive(next);
    refs.current[next]?.focus();
  };

  return (
    <div>
      <div
        role="tablist"
        aria-label="How an agent sees Sakura Lang"
        className="relative grid rounded-full border border-line bg-bg-sunk p-1"
        style={{ gridTemplateColumns: `repeat(${items.length}, minmax(0, 1fr))` }}
      >
        <span
          aria-hidden
          className="absolute inset-y-1 left-1 rounded-full bg-surface shadow-[0_1px_2px_oklch(0.2_0.02_255/0.12),0_0_0_1px_var(--line)] transition-transform duration-[250ms] ease-[var(--ease-out)]"
          style={{
            width: `calc((100% - 0.5rem) / ${items.length})`,
            transform: `translateX(${active * 100}%)`,
          }}
        />
        {items.map((it, i) => (
          <button
            key={it.label}
            ref={(el) => {
              refs.current[i] = el;
            }}
            role="tab"
            type="button"
            id={`${id}-tab-${i}`}
            aria-selected={active === i}
            aria-controls={`${id}-panel-${i}`}
            tabIndex={active === i ? 0 : -1}
            onClick={() => setActive(i)}
            onKeyDown={onKey}
            className={`relative z-10 truncate rounded-full px-2 py-2 text-[13px] font-medium transition-colors duration-200 sm:text-[14px] ${
              active === i ? "text-fg" : "text-fg-muted hover:text-fg"
            }`}
          >
            {it.short ? (
              <>
                <span className="sm:hidden">{it.short}</span>
                <span className="max-sm:hidden">{it.label}</span>
              </>
            ) : (
              it.label
            )}
          </button>
        ))}
      </div>
      {items.map((it, i) => (
        <div
          key={it.label}
          role="tabpanel"
          id={`${id}-panel-${i}`}
          aria-labelledby={`${id}-tab-${i}`}
          hidden={active !== i}
          className="mt-4 motion-safe:animate-[note-in_250ms_var(--ease-out)_both]"
        >
          {it.panel}
        </div>
      ))}
    </div>
  );
}
