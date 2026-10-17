# Sakura Lang website

The landing page and documentation for Sakura Lang, built with Next.js (App Router), Tailwind CSS v4 and Fumadocs.

## Run it

```bash
bun install
bun run dev      # http://localhost:3000
bun run build    # production build (type-checks)
bun run lint     # oxlint
bun run fmt      # oxfmt (fmt:check in CI)
bun run typecheck
bun scripts/check-grammar.ts   # smoke test for the Sakura Lang syntax grammar
```

Set `NEXT_PUBLIC_SITE_URL` to the deployed origin so OG images, the sitemap and `llms.txt` use absolute URLs. It defaults to `http://localhost:3000`.

## Layout

| Path                                            | What it is                                                                   |
| ----------------------------------------------- | ---------------------------------------------------------------------------- |
| `app/page.tsx`, `components/landing/`           | Landing page sections                                                        |
| `app/docs/`                                     | Fumadocs docs routes (`/docs/...`)                                           |
| `content/docs/`                                 | Docs content (MDX); order lives in `meta.json` files                         |
| `app/llms.txt`, `app/llms-full.txt`             | Agent-readable index and full text                                           |
| `app/llms.mdx/docs/[[...slug]]`                 | One page as Markdown; `/docs/<page>.md` is rewritten here                    |
| `lib/sakura/grammar.ts`, `lib/sakura/themes.ts` | TextMate grammar and Shiki themes for `sakura` code                          |
| `lib/sakura/highlight.ts`                       | Build-time tokenizer used by the landing page                                |
| `lib/source.ts`                                 | Fumadocs source, `llms.txt` builder and Markdown rendering of MDX components |
| `app/globals.css`                               | Design tokens (OKLCH) shared by landing and docs                             |

## Writing docs

- Code fences tagged `sakura` are highlighted with the Sakura Lang grammar.
- `Callout`, `Cards`/`Card` and `Steps`/`Step` are rendered to plain Markdown in the `.md` exports. Register any new component in `lib/source.ts` (`mdxAsPlaceholder` and `markdownFor`) so agents never see raw JSX.
- The language design document will land in `docs/` at the repo root; these pages should be synced to it when it does.
