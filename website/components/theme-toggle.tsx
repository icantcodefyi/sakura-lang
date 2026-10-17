"use client";

import { Moon, Sun } from "lucide-react";
import { useTheme } from "next-themes";

export function ThemeToggle() {
  const { resolvedTheme, setTheme } = useTheme();
  return (
    <button
      type="button"
      onClick={() => setTheme(resolvedTheme === "dark" ? "light" : "dark")}
      className="relative grid size-9 place-items-center rounded-full text-muted-foreground transition-[color,background-color,transform] duration-150 hover:bg-muted hover:text-foreground active:scale-[0.96]"
      aria-label="Toggle color theme"
    >
      <Sun
        className="size-4 scale-100 transition-[transform,opacity] duration-200 dark:scale-75 dark:opacity-0"
        strokeWidth={1.75}
      />
      <Moon
        className="absolute size-4 scale-75 opacity-0 transition-[transform,opacity] duration-200 dark:scale-100 dark:opacity-100"
        strokeWidth={1.75}
      />
    </button>
  );
}
