export const site = {
  name: "Vow",
  title: "Vow: the language agents write",
  tagline: "Code that keeps its promises.",
  description:
    "Vow is an agent-first programming language. Every signature declares its effects, errors and contracts, and the compiler holds the code to them.",
  // Set NEXT_PUBLIC_SITE_URL when a domain exists; used for OG, sitemap and llms.txt links.
  url: process.env.NEXT_PUBLIC_SITE_URL ?? "http://localhost:3000",
  github: "https://github.com/icantcodefyi/vow",
} as const;
