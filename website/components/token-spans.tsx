import type { CSSProperties } from "react";

export type Token = { text: string; light: string; dark: string; italic?: boolean };
export type TokenLine = Token[];

/** Renders highlighted tokens; colors switch with the theme via CSS vars. */
export function TokenSpans({ line, upTo }: { line: TokenLine; upTo?: number }) {
  let left = upTo ?? Infinity;
  const out = [];
  for (let i = 0; i < line.length && left > 0; i++) {
    const t = line[i];
    const text = t.text.length > left ? t.text.slice(0, left) : t.text;
    left -= text.length;
    out.push(
      <span
        key={i}
        className="tk"
        style={{ "--l": t.light, "--d": t.dark, fontStyle: t.italic ? "italic" : undefined } as CSSProperties}
      >
        {text}
      </span>,
    );
  }
  return out;
}
