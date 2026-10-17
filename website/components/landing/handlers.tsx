"use client";

import { useState } from "react";

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
  return (
    <div className="overflow-hidden rounded-2xl border border-line bg-surface">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line px-4 py-3 sm:px-6">
        <code
          key={mode}
          className="min-w-0 font-mono text-[11.5px] text-fg-muted sm:truncate sm:text-[12px] motion-safe:animate-[note-in_260ms_var(--ease-out)_both]"
        >
          {WITH[mode]}
        </code>
        <fieldset className="relative grid grid-cols-2 rounded-full border border-line bg-bg-sunk p-0.5 text-[12.5px] font-medium">
          <legend className="sr-only">Where signup runs</legend>
          <span
            aria-hidden
            className="absolute inset-y-0.5 left-0.5 w-[calc(50%-0.125rem)] rounded-full bg-surface shadow-[0_1px_2px_oklch(0.2_0.02_255/0.14),0_0_0_1px_var(--line)] transition-transform duration-[250ms] ease-[var(--ease-out)]"
            style={{ transform: mode === "test" ? "translateX(100%)" : "none" }}
          />
          {(["run", "test"] as const).map((m) => (
            <button
              key={m}
              type="button"
              aria-pressed={mode === m}
              onClick={() => setMode(m)}
              className={`relative z-10 h-9 rounded-full px-3.5 transition-colors duration-200 ${
                mode === m ? "text-fg" : "text-fg-muted hover:text-fg"
              }`}
            >
              {m === "run" ? "In production" : "In a test"}
            </button>
          ))}
        </fieldset>
      </div>

      <div className="grid grid-cols-3 gap-2 px-3 pt-6 sm:gap-6 sm:px-6 sm:pt-8">
        {LANES.map((lane, i) => {
          const h = lane[mode];
          return (
            <div key={lane.effect} className="flex min-w-0 flex-col items-center text-center">
              {/* the handler: the only thing that changes */}
              <div
                key={mode}
                className="w-full rounded-xl border border-seal/45 bg-seal-wash px-2 py-2.5 motion-safe:animate-[swap-in_320ms_var(--ease-out)_both] sm:px-3"
                style={{ animationDelay: `${i * 50}ms` }}
              >
                <p className="truncate font-mono text-[11px] font-medium text-fg sm:text-[12.5px]">
                  {h.handler}
                </p>
                <p className="mt-0.5 text-[11px] leading-snug text-fg-muted sm:text-[12.5px]">
                  {h.does}
                </p>
              </div>
              {/* the operation travels up to the innermost handler */}
              <div className="relative h-24 w-px bg-line-strong">
                <span
                  className="lane-dot absolute -left-1 size-2 rounded-full bg-seal"
                  style={{ animationDelay: `${i * 700}ms` }}
                />
                <span className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 rounded bg-surface px-1.5 py-0.5 font-mono text-[10.5px] whitespace-nowrap text-fg-muted sm:text-[11.5px]">
                  {lane.op}
                </span>
              </div>
            </div>
          );
        })}
      </div>

      {/* the function and its row: identical in both modes */}
      <div className="mx-3 mb-3 rounded-xl border border-line bg-code px-3 py-3 sm:mx-6 sm:mb-6 sm:px-5">
        <div className="grid grid-cols-3 gap-2 font-mono text-[11px] sm:gap-6 sm:text-[12.5px]">
          {LANES.map((lane) => (
            <span key={lane.effect} className="truncate text-center text-seal-text">
              {lane.effect.replace(/<.*/, "")}
              <span className="hidden sm:inline">{lane.effect.match(/<.*/)?.[0]}</span>
            </span>
          ))}
        </div>
        <p className="mt-2 border-t border-line pt-2 text-center font-mono text-[11.5px] text-fg-muted sm:text-[12.5px]">
          signup(email, pw) <span className="text-fg-faint">· the body is the same in both</span>
        </p>
      </div>
    </div>
  );
}
