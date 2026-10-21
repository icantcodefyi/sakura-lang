<!-- BEGIN:nextjs-agent-rules -->

## This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` (resolved from this file's directory; in monorepos the `next` package may not be visible from the repo root) before writing any code. Heed deprecation notices.

This block is written and re-added by `next dev` — verify at `node_modules/next/dist/server/lib/generate-agent-files.js`. Removing it from a diff only re-creates the uncommitted change; committing it with your work keeps the tree clean.

<!-- END:nextjs-agent-rules -->

## Landing page

The page answers, in order: what Sakura Lang is, why use it, how it works with a coding agent,
what the code looks like, whether it is fast, whether it is ready and how to start. Detail
belongs in the docs, linked from there.
Every displayed version comes from `RELEASE` in `lib/release.ts`: the hero badge, install
commands, the status section, footer, changelog page and llms.txt. Change it only once the tag
is published; then check that `status.tsx` still matches /docs/stability. `/changelog` renders
the repository `CHANGELOG.md`, and `vercel.json` redeploys when it changes.
Its Sakura samples live in `components/landing/samples/*.sak` and are checked by
`scripts/check-doc-sweep.py`; the outputs shown beside them (`code.tsx`, `install.tsx`,
`tests-figure.tsx`) are recorded from the compiler and must be re-recorded when they change.
Benchmark figures come only from `lib/bench.ts`.

The hero tree is grown from a fixed seed in `lib/sakura-tree.ts`. `app/tree-*.svg` serve it as a
poster per theme; `tree-scene.ts` (three.js, loaded by dynamic import only) draws the same tree
live through the same camera. Keep three.js out of every static import. The figures loop in CSS
(`globals.css`, Landing), only inside `play.tsx` and only without reduced motion.

## CI coverage

Website changes select the website build, documentation examples and Tree-sitter
corpus checks. The documentation job builds the current CLI and checks guide fences
and landing samples. Keep sample changes alongside the copy that displays them;
see [the CI policy](../CONTRIBUTING.md#ci-policy) for focused manual verification.
