"use client";

import { Check, Copy } from "lucide-react";
import { useEffect, useState } from "react";

/** Shell commands in one block; a click anywhere copies them all, unless the reader is selecting text. */
export function CopyCommand({ commands }: { commands: string[] }) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const t = setTimeout(() => setCopied(false), 1600);
    return () => clearTimeout(t);
  }, [copied]);

  async function copy() {
    if (window.getSelection()?.toString()) return;
    await navigator.clipboard.writeText(commands.join("\n"));
    setCopied(true);
  }

  return (
    // The button below is the keyboard and screen-reader control; this click target only widens it.
    <div
      role="presentation"
      onClick={copy}
      className="group relative min-w-0 cursor-pointer rounded-lg border border-border bg-card transition-colors duration-150 hover:border-strong"
    >
      <pre className="overflow-x-auto py-3.5 pr-14 pl-4 font-mono text-[12.5px] leading-[1.75] text-foreground">
        {commands.map((cmd) => (
          <code key={cmd} className="block whitespace-pre">
            {cmd.split("\n").map((line, i) => (
              <span key={i} className="block">
                <span aria-hidden className="text-faint select-none">
                  {i === 0 ? "$ " : "  "}
                </span>
                {line.trimStart()}
              </span>
            ))}
          </code>
        ))}
      </pre>
      <button
        type="button"
        aria-label="Copy commands"
        onClick={(e) => {
          e.stopPropagation();
          void copy();
        }}
        className="absolute top-2.5 right-2.5 grid size-8 place-items-center rounded-md text-muted-foreground transition-colors group-hover:text-foreground hover:bg-muted focus-visible:outline-2 focus-visible:outline-sakura"
      >
        <span className="t-icon-swap" data-state={copied ? "b" : "a"}>
          <Copy data-icon="a" className="size-4" />
          <Check data-icon="b" className="size-4 text-ok" />
        </span>
      </button>
      <span aria-live="polite" className="sr-only">
        {copied ? "Copied" : ""}
      </span>
    </div>
  );
}
