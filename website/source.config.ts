import { defineConfig } from "fumadocs-mdx/config";
import { sakuraGrammar } from "./lib/sakura/grammar";
import { sakuraDark, sakuraLight } from "./lib/sakura/themes";

export default defineConfig({
  mdxOptions: {
    rehypeCodeOptions: {
      themes: { light: sakuraLight, dark: sakuraDark },
      langs: [sakuraGrammar, "ts", "tsx", "json", "bash", "toml", "diff", "sql", "md"],
    },
  },
});
