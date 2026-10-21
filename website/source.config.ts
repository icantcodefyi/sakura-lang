import { defineConfig } from "fumadocs-mdx/config";
import { PETAL_ANGLES, SAKURA_CUTS } from "./components/brand";
import { sakuraGrammar } from "./lib/sakura/grammar";
import { sakuraDark, sakuraLight } from "./lib/sakura/themes";

// A titled Sakura block (a .sak file) shows the Sakura Seal beside its file name.
const seal = SAKURA_CUTS.small;
const sealIcon = `<svg viewBox="0 0 100 100" aria-hidden="true"><g transform="translate(0 ${seal.dy})"><g fill="var(--sakura)">${PETAL_ANGLES.map(
  (a) => `<path d="${seal.petal}"${a ? ` transform="rotate(${a} 50 50)"` : ""}/>`,
).join("")}</g><circle cx="50" cy="50" r="${seal.core}" fill="var(--magenta)"/></g></svg>`;

export default defineConfig({
  mdxOptions: {
    rehypeCodeOptions: {
      themes: { light: sakuraLight, dark: sakuraDark },
      langs: [sakuraGrammar, "ts", "tsx", "json", "bash", "toml", "diff", "sql", "md"],
      icon: { extend: { sakura: sealIcon } },
    },
  },
});
