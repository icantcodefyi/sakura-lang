import { createMDX } from "fumadocs-mdx/next";

/** @type {import('next').NextConfig} */
const config = {
  reactStrictMode: true,
  cacheComponents: true,
  partialPrefetching: true,
  turbopack: {
    rules: {
      "*.css": {
        loaders: [
          {
            loader: "@tailwindcss/turbopack",
            options: {
              // Lightning CSS falsely warns on Fumadocs' valid ::highlight(fd-search) rules:
              // https://github.com/parcel-bundler/lightningcss/issues/1300
              // Let Turbopack minify the CSS instead of running Tailwind's extra optimizer.
              optimize: false,
            },
          },
        ],
        as: "*.css",
      },
    },
  },
  async rewrites() {
    return [
      // Agents can append .md to any docs URL to get raw Markdown.
      { source: "/docs.md", destination: "/llms.mdx/docs/content.md" },
      { source: "/docs/:slug*.md", destination: "/llms.mdx/docs/:slug*/content.md" },
      { source: "/docs/:slug*.mdx", destination: "/llms.mdx/docs/:slug*/content.md" },
    ];
  },
};

const withMDX = createMDX();

export default withMDX(config);
