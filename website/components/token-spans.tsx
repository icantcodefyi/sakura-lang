import type { CSSProperties } from "react";

export type Token = { text: string; light: string; dark: string; italic?: boolean };
export type TokenLine = Token[];

/** Renders highlighted tokens, characters `from` to `upTo`; colors switch with the theme via CSS vars. */
export function TokenSpans({
  line,
  from = 0,
  upTo,
}: {
  line: TokenLine;
  from?: number;
  upTo?: number;
}) {
  let skip = from;
  let left = (upTo ?? Infinity) - from;
  const out = [];
  for (let i = 0; i < line.length && left > 0; i++) {
    const t = line[i];
    if (skip >= t.text.length) {
      skip -= t.text.length;
      continue;
    }
    const rest = t.text.slice(skip);
    skip = 0;
    const text = rest.length > left ? rest.slice(0, left) : rest;
    left -= text.length;
    out.push(
      <span
        key={i}
        className="tk"
        style={
          {
            "--l": t.light,
            "--d": t.dark,
            fontStyle: t.italic ? "italic" : undefined,
          } as CSSProperties
        }
      >
        {text}
      </span>,
    );
  }
  return out;
}
