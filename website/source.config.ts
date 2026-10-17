import { defineConfig } from "fumadocs-mdx/config";
import { vowGrammar } from "./lib/vow/grammar";
import { vowDark, vowLight } from "./lib/vow/themes";

export default defineConfig({
  mdxOptions: {
    rehypeCodeOptions: {
      themes: { light: vowLight, dark: vowDark },
      langs: [vowGrammar, "ts", "tsx", "json", "bash", "toml", "diff", "sql", "md"],
    },
  },
});
