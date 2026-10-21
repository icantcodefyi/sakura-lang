"use client";

import { useState } from "react";
import { useSlidingPill } from "@/components/sliding-pill";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { WINDOW, WindowBar } from "./window";

type Mode = "run" | "test";

/** One lane per label in signup's row: the operation goes up, the installed handler answers. */
const LANES = [
  {
    effect: "Db",
    op: "Db.exists(…)",
    run: { handler: "sql_db()", does: "clauses call Sql.sqlite" },
    test: { handler: "memory_db()", does: "a Map in handler state" },
  },
  {
    effect: "Clock",
    op: "Clock.now()",
    run: { handler: "Clock.host()", does: "reads the system clock" },
    test: { handler: "Clock.fixed(t0)", does: "always 2026-01-01" },
  },
  {
    effect: "Fail<SignupError>",
    op: "fail EmailTaken",
    run: { handler: "try/catch", does: "turns it into a 409" },
    test: { handler: "expect_fail", does: "asserts the tag" },
  },
] as const;

const WITH = {
  run: "with sql_db() { Serve.listen(router, addr) }",
  test: 'test "duplicate email" { with memory_db() { … } }',
} as const;

export function HandlersDiagram() {
  const [mode, setMode] = useState<Mode>("run");
  const { bar, pill, ready } = useSlidingPill(mode);
  return (
    <div className={WINDOW}>
      <WindowBar className="flex-wrap justify-between gap-x-3 gap-y-2 py-2 sm:px-4">
        <code
          key={mode}
          className="min-w-0 text-muted-foreground sm:truncate sm:text-[12px] motion-safe:animate-[note-in_var(--duration-fast)_var(--ease-out)_both]"
        >
          {WITH[mode]}
        </code>
        <ToggleGroup
          type="single"
          value={mode}
          onValueChange={(v) => v && setMode(v as Mode)}
          aria-label="Where signup runs"
          spacing={0}
          ref={bar}
          data-pill={ready || undefined}
          className="group/pill relative rounded-md border border-border bg-background p-0.5 font-sans"
        >
          <span
            ref={pill}
            aria-hidden
            className="t-tabs-pill hidden rounded-[5px] bg-paper group-data-pill/pill:block"
          />
          {(["run", "test"] as const).map((m) => (
            <ToggleGroupItem
              key={m}
              value={m}
              className="z-[1] h-8 rounded-[5px]! px-3 text-[12.5px] text-muted-foreground data-[state=on]:bg-paper data-[state=on]:text-ink group-data-pill/pill:data-[state=on]:bg-transparent"
            >
              {m === "run" ? "In production" : "In a test"}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </WindowBar>

      <div className="grid grid-cols-3 gap-2 px-3 pt-6 sm:gap-6 sm:px-6 sm:pt-8">
        {LANES.map((lane, i) => {
          const h = lane[mode];
          return (
            <div key={lane.effect} className="flex min-w-0 flex-col items-center text-center">
              {/* the handler: the only thing that changes */}
              <div
                key={mode}
                className="w-full rounded-md border border-sakura/60 bg-sakura-wash px-2 py-2.5 motion-safe:animate-[swap-in_var(--duration-fast)_var(--ease-out)_both] sm:px-3"
                style={{ animationDelay: `calc(${i} * var(--duration-stagger))` }}
              >
                <p className="font-mono text-[11px] font-medium text-foreground sm:text-[12.5px]">
                  {/* a zero-width space lets Clock.fixed(t0) break after the dot on phones */}
                  {h.handler.replace(".", ".\u200b")}
                </p>
                <p className="mt-0.5 text-[11px] leading-snug text-muted-foreground sm:text-[12.5px]">
                  {h.does}
                </p>
              </div>
              {/* the operation travels up to the innermost handler */}
              <div className="relative h-24 w-px bg-strong">
                <span
                  className="lane-dot absolute -left-1 size-2 rounded-full bg-sakura"
                  style={{ animationDelay: `${i * 700}ms` }}
                />
                <span className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 rounded bg-card px-1.5 py-0.5 font-mono text-[10.5px] whitespace-nowrap text-muted-foreground sm:text-[11.5px]">
                  {lane.op}
                </span>
              </div>
            </div>
          );
        })}
      </div>

      {/* the function and its row: identical in both modes */}
      <div className="mx-3 mb-3 rounded-md border border-border bg-code px-3 py-3 sm:mx-6 sm:mb-6 sm:px-5">
        <div className="grid grid-cols-3 gap-2 font-mono text-[11px] sm:gap-6 sm:text-[12.5px]">
          {LANES.map((lane) => (
            <span key={lane.effect} className="truncate text-center text-sakura-text">
              {lane.effect.replace(/<.*/, "")}
              <span className="hidden sm:inline">{lane.effect.match(/<.*/)?.[0]}</span>
            </span>
          ))}
        </div>
        <p className="mt-2 border-t border-border pt-2 text-center font-mono text-[11.5px] text-muted-foreground sm:text-[12.5px]">
          signup(email, pw) <span className="text-faint">· the body is the same in both</span>
        </p>
      </div>
    </div>
  );
}
