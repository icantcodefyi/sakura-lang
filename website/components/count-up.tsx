"use client";

import { useEffect, useLayoutEffect, useRef } from "react";

// useLayoutEffect warns during the server render; the effect only matters in the browser.
const useBrowserLayoutEffect = typeof window === "undefined" ? useEffect : useLayoutEffect;

/**
 * A figure such as `3.46×` or `54 ms` that counts up from zero the first time it scrolls into
 * view. The server renders the final text, and an invisible copy of it keeps the box at its
 * final width, so counting never reflows anything. Screen readers get the final text only.
 * Without motion (reduced motion, or no JS) the final figure simply stays.
 */
export function CountUp({ value }: { value: string }) {
  const out = useRef<HTMLSpanElement>(null);
  useBrowserLayoutEffect(() => {
    const el = out.current;
    const m = value.match(/^(\d+(?:\.\d+)?)(.*)$/);
    if (!el || !m || matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const target = Number(m[1]);
    const places = m[1].split(".")[1]?.length ?? 0;
    const show = (v: number) => (el.textContent = v.toFixed(places) + m[2]);
    // Below the fold on load, so the reset to zero is never seen.
    if (el.getBoundingClientRect().top < window.innerHeight) return;
    show(0);
    let raf = 0;
    const io = new IntersectionObserver(
      ([e]) => {
        if (!e.isIntersecting) return;
        io.disconnect();
        const start = performance.now();
        const step = (now: number) => {
          const k = Math.min(1, (now - start) / 1100);
          show(target * (1 - (1 - k) ** 4)); // ease-out quart
          if (k < 1) raf = requestAnimationFrame(step);
        };
        raf = requestAnimationFrame(step);
      },
      { threshold: 0.6 },
    );
    io.observe(el);
    return () => {
      io.disconnect();
      cancelAnimationFrame(raf);
    };
  }, [value]);

  return (
    <span className="inline-grid tabular-nums">
      <span className="invisible col-start-1 row-start-1" aria-hidden>
        {value}
      </span>
      <span ref={out} className="col-start-1 row-start-1" aria-hidden>
        {value}
      </span>
      <span className="sr-only">{value}</span>
    </span>
  );
}
