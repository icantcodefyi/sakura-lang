import type { ReactNode } from "react";
import { tokenize } from "@/lib/sakura/highlight";
import { TokenSpans } from "./token-spans";

type CodeProps = {
  code: string;
  lang?: string;
  title?: ReactNode;
  /** 0-based line indexes to mark */
  mark?: Record<number, "add" | "del" | "seal" | "fail" | "ok">;
  className?: string;
  numbers?: boolean;
  /** soft-wrap long lines instead of scrolling */
  wrap?: boolean;
};

const markClass = {
  add: "bg-ok-wash",
  del: "bg-fail-wash",
  seal: "bg-sakura-wash",
  fail: "bg-fail-wash",
  ok: "bg-ok-wash",
} as const;

/** Static, server-highlighted code block in the site's code style. */
export async function Code({
  code,
  lang = "sakura",
  title,
  mark,
  className,
  numbers = false,
  wrap = false,
}: CodeProps) {
  const lines = await tokenize(code.replace(/\n$/, ""), lang);
  return (
    <figure
      className={`overflow-hidden rounded-xl border border-border bg-code ${className ?? ""}`}
    >
      {title ? (
        <figcaption className="flex items-center gap-2 border-b border-border px-4 py-2 font-mono text-[11.5px] text-muted-foreground">
          {title}
        </figcaption>
      ) : null}
      <pre className="overflow-x-auto py-3 font-mono text-[12.5px] leading-[1.7]">
        <code
          className={`grid ${wrap ? "whitespace-pre-wrap [overflow-wrap:anywhere]" : "min-w-max"}`}
        >
          {lines.map((line, i) => (
            <span key={i} className={`flex px-4 ${mark?.[i] ? markClass[mark[i]] : ""}`}>
              {numbers ? (
                <span
                  aria-hidden
                  className={`mr-4 w-5 shrink-0 text-right tabular-nums select-none ${mark?.[i] ? "text-muted-foreground" : "text-faint"}`}
                >
                  {i + 1}
                </span>
              ) : null}
              <span>
                <TokenSpans line={line} />
                {line.length === 0 ? " " : null}
              </span>
            </span>
          ))}
        </code>
      </pre>
    </figure>
  );
}
