"use client";

import { useLayoutEffect, useRef, useState } from "react";

/**
 * The active pill of a segmented control, slid between options (transitions.dev "tabs sliding").
 * Put `bar` on the list and `pill` on an absolutely positioned `.t-tabs-pill` inside it; the
 * active option is the child with Radix's `data-state="active"` or `"on"`. Until the pill is
 * placed, `ready` is false and the active option keeps its own background, so the server render
 * and the first paint show the same thing. The first placement and every resize snap without a
 * transition; a change of `active` slides.
 */
export function useSlidingPill(active: string) {
  const bar = useRef<HTMLDivElement>(null);
  const pill = useRef<HTMLSpanElement>(null);
  const [ready, setReady] = useState(false);
  const placed = useRef(false);

  useLayoutEffect(() => {
    const b = bar.current;
    const p = pill.current;
    if (!b || !p) return;
    const place = (animate: boolean) => {
      const tab = b.querySelector<HTMLElement>('[data-state="active"], [data-state="on"]');
      if (!tab) return;
      // Rects, not offsetLeft/offsetWidth, which round to whole pixels in a fractional grid.
      const t = tab.getBoundingClientRect();
      const o = b.getBoundingClientRect();
      if (!animate) p.style.transition = "none";
      p.style.transform = `translate(${t.left - o.left - b.clientLeft}px, ${t.top - o.top - b.clientTop}px)`;
      p.style.width = `${t.width}px`;
      p.style.height = `${t.height}px`;
      if (!animate) {
        void p.offsetWidth;
        p.style.transition = "";
      }
    };
    place(placed.current);
    placed.current = true;
    setReady(true);
    const resize = new ResizeObserver(() => place(false));
    resize.observe(b);
    return () => resize.disconnect();
    // `active` is read from the DOM (Radix's data-state), so it is a dependency only as a trigger.
    // oxlint-disable-next-line react/exhaustive-effect-dependencies
  }, [active]);

  return { bar, pill, ready };
}
