"use client";

import { useEffect, useRef, type ComponentProps } from "react";

/**
 * Runs the CSS animations inside it only while it is on screen: it sets `data-play` from an
 * IntersectionObserver, and globals.css pauses every animation under `[data-gate]` without it.
 * The attribute is written straight to the DOM, so scrolling never re-renders anything.
 */
export function Play(props: ComponentProps<"div">) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const io = new IntersectionObserver(([e]) => el.toggleAttribute("data-play", e.isIntersecting));
    io.observe(el);
    return () => io.disconnect();
  }, []);
  return <div ref={ref} data-gate="" {...props} />;
}
