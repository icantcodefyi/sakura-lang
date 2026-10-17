export const site = {
  name: "Sakura Lang",
  title: "Sakura Lang: the language agents write",
  tagline: "Code that keeps its promises.",
  description:
    "Sakura Lang is an agent-first programming language. Every signature declares its effects, errors and contracts, and the compiler seals them on every build.",
  // Set NEXT_PUBLIC_SITE_URL when a domain exists; used for OG, sitemap and llms.txt links.
  url: process.env.NEXT_PUBLIC_SITE_URL ?? "http://localhost:3000",
  github: "https://github.com/icantcodefyi/sakura-lang",
} as const;
