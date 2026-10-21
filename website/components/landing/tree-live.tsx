"use client";

import { useEffect, useRef, type ReactNode } from "react";

/** Whether this visitor gets the live tree; otherwise the poster stays. */
function canAfford() {
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) return false;
  // Phones keep the poster: the scene is decoration, and their battery and data are not ours.
  if (!matchMedia("(min-width: 768px) and (pointer: fine)").matches) return false;
  const nav = navigator as Navigator & { connection?: { saveData?: boolean } };
  if (nav.connection?.saveData || (nav.hardwareConcurrency ?? 0) <= 4) return false;
  return !!document.createElement("canvas").getContext("webgl2");
}

/**
 * The poster (children) at the reserved size, then, once the page is idle and the hero is on
 * screen, three.js loaded by dynamic import and the live scene faded in over it. The scene
 * renders only while it is on screen and the tab is visible, and is disposed on unmount.
 */
export function TreeLive({ children, className }: { children: ReactNode; className?: string }) {
  const box = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const el = box.current;
    const cv = canvas.current;
    if (!el || !cv || !canAfford()) return;
    let scene: ReturnType<typeof import("./tree-scene").mount> | undefined;
    let dead = false;
    let onScreen = false;
    const sync = () => scene?.run(onScreen && !document.hidden);
    const load = () =>
      import("./tree-scene").then(({ mount }) => {
        if (dead) return;
        scene = mount(cv, () => el.setAttribute("data-live", ""));
        sync();
      });
    let idle = 0;
    const ric = (window as Partial<Window>).requestIdleCallback; // not in older Safari
    const io = new IntersectionObserver(([e]) => {
      onScreen = e.isIntersecting;
      if (onScreen && !scene && !idle)
        idle = ric
          ? ric(() => void load(), { timeout: 2500 })
          : window.setTimeout(() => void load(), 1200);
      sync();
    });
    io.observe(el);
    document.addEventListener("visibilitychange", sync);
    return () => {
      dead = true;
      io.disconnect();
      document.removeEventListener("visibilitychange", sync);
      scene?.dispose();
    };
  }, []);

  return (
    <div ref={box} className={`group/tree relative ${className ?? ""}`}>
      <div className="absolute inset-0 transition-opacity duration-700 ease-[var(--ease-out)] group-data-live/tree:opacity-0">
        {children}
      </div>
      <canvas
        ref={canvas}
        aria-hidden="true"
        className="absolute inset-0 size-full opacity-0 transition-opacity duration-700 ease-[var(--ease-out)] group-data-live/tree:opacity-100"
      />
    </div>
  );
}
