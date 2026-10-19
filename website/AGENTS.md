<!-- BEGIN:nextjs-agent-rules -->

## This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` (resolved from this file's directory; in monorepos the `next` package may not be visible from the repo root) before writing any code. Heed deprecation notices.

This block is written and re-added by `next dev` — verify at `node_modules/next/dist/server/lib/generate-agent-files.js`. Removing it from a diff only re-creates the uncommitted change; committing it with your work keeps the tree clean.

<!-- END:nextjs-agent-rules -->

## Landing page

When a user-visible milestone lands, update the landing page's status/roadmap strip: the hero
badge (`components/landing/hero.tsx`), the install rows (`install.tsx`), the agent list
(`agent-view.tsx`) and the release strip (`roadmap.tsx`), so the page shows only what has shipped.
Its Sakura samples live in `components/landing/samples/*.sak` and are checked by
`scripts/check-doc-sweep.py`; the outputs shown beside them are recorded from the compiler and
must be re-recorded when they change.
