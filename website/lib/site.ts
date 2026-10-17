export const site = {
  name: "Sakura Lang",
  title: "Sakura Lang: the language agents write",
  tagline: "Code that keeps its promises.",
  description:
    "Sakura Lang is an agent-first programming language. Every signature declares its effects, errors and contracts, and the compiler seals them on every build.",
  // The canonical origin for metadata, OG images, the sitemap and llms.txt links. Vercel production
  // also sets NEXT_PUBLIC_SITE_URL, which wins: update both when the domain moves.
  url: process.env.NEXT_PUBLIC_SITE_URL ?? "https://sakura-lang.vercel.app",
  github: "https://github.com/icantcodefyi/sakura-lang",
  license: "MIT OR Apache-2.0",
  author: {
    name: "Aniruddh Dubge",
    email: "icantcodefyi@gmail.com",
    handle: "@icantcodefyi",
    github: "https://github.com/icantcodefyi",
    x: "https://x.com/icantcodefyi",
    linkedin: "https://www.linkedin.com/in/aniruddhdubge/",
  },
} as const;
