<!-- BEGIN:nextjs-agent-rules -->

## This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` (resolved from this file's directory; in monorepos the `next` package may not be visible from the repo root) before writing any code. Heed deprecation notices.

This block is written and re-added by `next dev` — verify at `node_modules/next/dist/server/lib/generate-agent-files.js`. Removing it from a diff only re-creates the uncommitted change; committing it with your work keeps the tree clean.

<!-- END:nextjs-agent-rules -->

## Landing page

Every displayed version comes from `RELEASE` in `lib/release.ts`: the hero badge, install
commands and platform rows, release strip, footer, changelog page and llms.txt. Change it only
once the tag is published; then check that the copy beside it (`install.tsx` platform `since`,
`roadmap.tsx` bodies, `agent-view.tsx` list) still shows only what has shipped. `/changelog`
renders the repository `CHANGELOG.md`, and `vercel.json` redeploys when it changes.
Its Sakura samples live in `components/landing/samples/*.sak` and are checked by
`scripts/check-doc-sweep.py`; the outputs shown beside them are recorded from the compiler and
must be re-recorded when they change.

## CI coverage

Website changes select the website build, documentation examples and Tree-sitter
corpus checks. The documentation job builds the current CLI and checks guide fences
and landing samples. Keep sample changes alongside the copy that displays them;
see [the CI policy](../CONTRIBUTING.md#ci-policy) for focused manual verification.
