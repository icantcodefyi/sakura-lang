import type { CSSProperties } from "react";
import { Play } from "./play";

// The default handlers under `sakura test` (docs/tour/testing, "Tests swap handlers"); under
// `sakura run`, root handlers come from the host. Any other effect in a test needs a `with`, or
// the compiler reports V3020.
const ROWS = [
  { effect: "Clock", run: "host clock", test: "Clock.fixed(…)" },
  { effect: "Random", run: "host", test: "Random.seeded(seed)" },
  { effect: "Env", run: "host", test: "Env.of(Map.empty())" },
  { effect: "Console", run: "stdout", test: "Console.captured()" },
  { effect: "Http", run: "host, if granted", test: "V3020 without a with", fail: true },
];

/**
 * The same function's effects, answered by the host under `sakura run` and by deterministic
 * handlers under `sakura test`. The two answers swap in place on a loop (CSS `det-*`, opacity
 * and transform only); both are laid out at once, so nothing reflows. The static frame is the
 * test column.
 */
export function TestsFigure() {
  return (
    <Play>
      <figure>
        <div
          aria-hidden
          className="relative mb-2 grid w-max grid-cols-2 rounded-md border border-border p-0.5 font-mono text-[12px]"
        >
          <span className="det-pill absolute inset-y-0.5 left-0.5 w-[calc(50%-2px)] rounded-[5px] bg-sakura-wash" />
          <span className="relative px-3 py-1 text-center">sakura run</span>
          <span className="relative px-3 py-1 text-center">sakura test</span>
        </div>
        <table className="w-full border-collapse text-left">
          <caption className="sr-only">
            Handlers for each effect: under sakura run the host answers; under sakura test the
            handlers are deterministic.
          </caption>
          <thead className="sr-only">
            <tr>
              <th scope="col">Effect</th>
              <th scope="col">sakura run</th>
              <th scope="col">sakura test</th>
            </tr>
          </thead>
          <tbody className="border-t border-border">
            {ROWS.map((r, i) => (
              <tr
                key={r.effect}
                className="border-b border-border"
                style={{ "--det-i": i } as CSSProperties}
              >
                <th
                  scope="row"
                  className="w-20 py-2.5 pr-3 font-mono text-[12.5px] font-normal text-sakura-text"
                >
                  {r.effect}
                </th>
                <td className="w-[22%] px-1">
                  <span aria-hidden className="relative block h-px bg-border">
                    {/* the track moves, so the dot crosses the cell whatever its width */}
                    <span className="det-dot absolute inset-0">
                      <span className="absolute -top-[3px] -left-[3.5px] size-[7px] rounded-full bg-sakura" />
                    </span>
                  </span>
                </td>
                <td
                  aria-label={`sakura run: ${r.run}; sakura test: ${r.test}`}
                  className="py-2.5 pl-3 font-mono text-[12.5px]"
                >
                  <span aria-hidden className="grid">
                    <span className="det-run col-start-1 row-start-1 text-muted-foreground">
                      {r.run}
                    </span>
                    <span
                      className={`det-test col-start-1 row-start-1 ${r.fail ? "text-fail" : "text-foreground"}`}
                    >
                      {r.test}
                    </span>
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <figcaption className="mt-3 flex items-center gap-2 font-mono text-[12px] text-ok">
          <svg viewBox="0 0 12 12" className="size-3" aria-hidden>
            <path
              d="M2.5 6.3 5 8.6l4.5-5"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.8"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
          1 passed, 0 failed, 0 skipped (seed 0)
        </figcaption>
      </figure>
    </Play>
  );
}
