# Website research gaps for the six dogfood briefs

Research date: 2026-10-19. Research/brief author: GPT-6-Astra (Codex family).
There are **13 items**, all authored by GPT-6-Astra. These are documentation
observations and implementation questions, not demonstrated compiler defects.
No code, `.sak` files, test fixtures or migration lists were authored in this pass.

## Evidence boundary and handoff

The research read the requested decisions in `docs/design.md`, stage 7 of
`docs/plans/road-to-1.0.md`, and `AGENTS.md`. After the worktree was updated to
include decision 17.255, it also read the last line of `compiler/REQUESTS.md`,
explicitly requested by the lead. That line supplied role/workflow instructions,
not language implementation evidence. No compiler, std, tests, website source,
repository ABI, other language reference source or linked GitHub example was read
to answer a language question. All API/language research used public
`https://www.sakuralang.com` pages and their `.md` exports. The site-wide export
was searched for embedding documentation. The public changelog was read for the
published alpha.1 baseline and changes since it.

The local worktree initially ended at decision 17.254; fetching and rebasing onto
`origin/main` supplied 17.255 at `839a2b79`. That resolved workspace issue and a web
search tool's inability to fetch the homepage are not counted as website defects;
the collaborative browser successfully read the public site.

All six programs remain Opus's work, each in its own fresh session without a
repository checkout or path to one. Give each author only its brief and permitted
published materials, not this research session's repository checkout. At least
300 non-test `.sak` lines, 20 separately named tests and service cassettes remain
required per program. The briefs specify application policy; they do not change
language semantics or assert that any acceptance case has passed.

Start against installed `v0.1.0-alpha.1`, which the install page offers for Linux
x64 and Apple silicon macOS. This Windows research worktree is not evidence of an
alpha.1 Windows installation: the site explicitly says there is none. Record each
implementation's actual release/build and platform. Acceptance is against installed
`v0.1.0` on both JS and WASM. After 1.0.0-beta.1 tags, Astra writes each release-notes-only
`MIGRATION.md` before Opus edits. The corpus is six Opus-authored programs with
Astra's briefs, reader reviews and migration lists; it is not two-family program
authorship. The human contributor/lead owns the result and review.

## G01 — The published-release path repeatedly leads back to main

- Date: 2026-10-19. Author: GPT-6-Astra. Status: open documentation friction.
- Evidence: [site index](https://www.sakuralang.com/llms.txt), [install](https://www.sakuralang.com/docs/install), [cookbook](https://www.sakuralang.com/docs/cookbook), [changes](https://www.sakuralang.com/docs/changes).
- Observation: the site correctly labels alpha.1 as published and docs as tracking
  main, but cookbook provenance tells the reader to build the checkout's CLI and
  validates recipes against that checkout. It supplies a historical revision, not
  an alpha.1 compatibility result for each recipe. The changes page likewise points
  to a source build for current features.
- Impact: every fresh author must distinguish an unsupported alpha.1 feature from
  an incorrect program without consulting source. A successful current-site recipe
  is not installed-alpha.1 evidence.
- Requested docs: a release-pinned website/recipe view or a per-recipe first-release
  and tested-target statement, with an installed-binary path. Keep main's docs too.
- Brief handling: all six name alpha.1 as the initial baseline, record failures,
  forbid source-build substitution and defer installed-0.1.0 acceptance honestly.

## G02 — No public Node library/pull-boundary consumption recipe found

- Date: 2026-10-19. Author: GPT-6-Astra. Status: blocking the required embedding integration.
- Evidence: [backends](https://www.sakuralang.com/docs/tour/memory#backends), [CLI](https://www.sakuralang.com/docs/cli), [capabilities](https://www.sakuralang.com/docs/tour/capabilities), [full site export](https://www.sakuralang.com/llms-full.txt).
- Observation: the pages document `build/run/test --target wasm`, a `lib` project
  kind and emitted Node host files. They do not provide a callable Node library
  entry, argument/result/error marshalling, pending-operation/resumption protocol,
  cleanup or embedding permission setup. Searches for pull boundary, instantiate,
  embedding, entrypoint and Node host found no end-to-end consumption recipe. The
  site links a repository ABI, outside this research boundary; it was not opened.
- Impact: the required fifth program cannot be specified down to an honest public
  invocation mechanism from the website alone. CLI execution is not equivalent.
- Requested docs: an installed-release example of a Node consumer invoking a Sakura
  library, suspending for host work and resuming, including authority, matching
  host/module versions, failure and cleanup. If that use is unavailable, say so
  explicitly so the lead can resolve the gate without inventing an API.
- Brief handling: [Parcel Quotes](parcel-quotes/BRIEF.md) retains the requirement
  and flags the blocker; it does not prescribe fictional exports or a custom ABI.

## G03 — Persistent SQLite setup and restart testing lack a website recipe

- Date: 2026-10-19. Author: GPT-6-Astra. Status: open documentation gap.
- Evidence: [SQLite recipe](https://www.sakuralang.com/docs/cookbook/sqlite_query), [std SQL](https://www.sakuralang.com/docs/stdlib#stdsql), [manifest](https://www.sakuralang.com/docs/manifest).
- Observation: the working recipe uses `Sql.memory`; persistent use is referred to
  `sakura doc` plus an appropriately confined handler and permissions. The public
  signature gives `Sql.sqlite(Path)` but no complete persistent-service example
  tying directory root, file path, read/write grants, startup and restart together.
  Ordinary tests have no host permissions, so replacing the in-memory constructor
  with a host database inside a test is not a documented persistence test.
- Impact: Repair Queue needs evidence that the real file survives restart, beyond
  its pure policy and in-memory SQL suite.
- Requested docs: a confined persistent SQLite recipe, its exact manifest and a
  separate installed-binary restart test procedure that preserves ordinary test denial.
- Brief handling: memory tests and persistent integration acceptance are explicitly
  separate; no undocumented temporary-directory test capability is assumed.

## G04 — Capability names disagree across the public reference

- Date: 2026-10-19. Author: GPT-6-Astra. Status: open reference inconsistency.
- Evidence: [capabilities](https://www.sakuralang.com/docs/tour/capabilities#narrowing-authority), [error index in llms.txt](https://www.sakuralang.com/llms.txt).
- Observation: the narrowing guide explicitly says `NetScope`, `EnvScope` and
  `SecretStore` are not current APIs. The public error index's V6001 description
  lists those names alongside `Dir` as capability values users cannot construct.
- Impact: a reader designing a least-authority program may search for nonexistent
  capability constructors instead of using the documented `Http.scoped` and
  `Env.only` handlers.
- Requested docs: distinguish implemented public types from design-only names in
  the generated error explanation/index, and point to the current narrowing APIs.
- Brief handling: only documented current narrowing APIs are required. No program
  depends on the three unavailable scope/store types.

## G05 — The service-testing path stops before Serve and real listener lifecycle

- Date: 2026-10-19. Author: GPT-6-Astra. Status: open documentation gap.
- Evidence: [router recipe](https://www.sakuralang.com/docs/cookbook/router_endpoint), [std Serve](https://www.sakuralang.com/docs/stdlib#stdserve), [testing](https://www.sakuralang.com/docs/tour/testing).
- Observation: the router recipe deliberately uses no listener. The std index lists
  `Serve.scripted(requests)` returning a test handler and a serving operation with
  result `Never`, without explaining how scripted serving terminates or exposes
  responses for assertions. No inspected page joins this with a persistent
  SQLite server and a start/stop/restart acceptance procedure.
- Impact: a direct route test or replay cassette cannot alone establish that an
  actual server listens, persists state and restarts correctly.
- Requested docs: a server test recipe explaining scripted response observation,
  finite test lifecycle and a separate real-listener smoke using published artifacts.
- Brief handling: Repair Queue requires router/database assertions and separate
  installed-binary listener/restart evidence; it does not invent a Serve capture API.

## G06 — Budget kinds in the concurrency prose exceed the documented API

- Date: 2026-10-19. Author: GPT-6-Astra. Status: open reference inconsistency.
- Evidence: [concurrency](https://www.sakuralang.com/docs/tour/concurrency#timeouts-retries-and-budgets), [std Budget](https://www.sakuralang.com/docs/stdlib#stdbudget).
- Observation: concurrency prose lists Bytes and Time alongside Tokens, Usd and
  Calls as budget-exceeded kinds. The std declaration has only Tokens, Usd and
  Calls, and `Limits` provides only tokens, usd and calls fields.
- Impact: the pipeline and agent cannot derive a byte/time-budget configuration
  from those promises or know which failure tags to match.
- Requested docs: reconcile the prose with the published `Limits`/`BudgetKind`
  surface and distinguish explicit stream limits and task timeouts.
- Brief handling: HTTP bodies have explicit byte caps, timeouts use Task, and the
  agent asks only for the three documented budget dimensions.

## G07 — Retry deadline wording can be mistaken for an in-flight timeout

- Date: 2026-10-19. Author: GPT-6-Astra. Status: open explanation friction.
- Evidence: [concurrency](https://www.sakuralang.com/docs/tour/concurrency#timeouts-retries-and-budgets), [retry recipe](https://www.sakuralang.com/docs/cookbook/http_retry).
- Observation: the tour describes the retry deadline as bounding total time; the
  concrete recipe clarifies that it bounds retry scheduling, not an individual
  request's duration. Reading only the tour leaves the stronger interpretation open.
- Impact: a data pipeline can appear bounded while one HTTP request remains pending.
- Requested docs: put the in-flight limitation beside the tour's policy description
  and demonstrate composition with a cooperative Task timeout.
- Brief handling: Supplier Stock specifies finite attempts and a separate timeout
  across each supplier's complete request/retry sequence.

## G08 — Convenient LLM replay can silently bypass the intended finite budget

- Date: 2026-10-19. Author: GPT-6-Astra. Status: documented but costly discovery friction.
- Evidence: [cassettes](https://www.sakuralang.com/docs/agents/cassettes#llm-replay), [budgeted agent](https://www.sakuralang.com/docs/cookbook/budgeted_agent), [concurrency](https://www.sakuralang.com/docs/tour/concurrency).
- Observation: the cassette page says `Llm.replay` installs an unlimited budget.
  The budgeted-agent recipe explains the correct finite-budget HTTP replay nesting,
  that `Llm.recording` also installs an unlimited budget, and why a two-turn example
  needs three simultaneous call reservations. This critical composition detail is
  easy to miss when arriving through the shorter concurrency/budget guide.
- Impact: a budget test can pass without testing the intended ceiling, or a user
  can read a two-call limit as permission for two provider turns.
- Requested docs: add a prominent cross-link/warning at the budget guide and both
  helper signatures, plus a concise explanation of reservation versus settled usage.
- Brief handling: Maintenance Triage specifies the documented finite-budget nesting
  and separate tests at the two/three-reservation boundary; no billing guarantee.

## G09 — Verification tour and reference disagree about removed checks

- Date: 2026-10-19. Author: GPT-6-Astra. Status: open current-doc inconsistency.
- Evidence: [verification tour](https://www.sakuralang.com/docs/tour/verification#what-changes-in-a-build), [verification reference](https://www.sakuralang.com/docs/agents/verify#proof-cache-and-build-behavior), [changes](https://www.sakuralang.com/docs/changes#webassembly-backend-complete).
- Observation: the tour says WASM builds retain the checks and that the predicate's
  computation remains. The reference describes proof-backed removal on WASM and
  removal of supported predicate computation, with an exception for function calls;
  the changes page also describes WASM removal.
- Impact: readers cannot infer the actual target-specific build guarantees from
  the introductory page, or safely report a check as removed from a proof alone.
- Requested docs: reconcile both pages, marking alpha.1 behaviour separately from
  main/0.1.0 and keeping the exact retained-check qualifications together.
- Brief handling: Workshop Power accepts named proof records and tests, not an
  optimisation claim; no check-removal performance conclusion is required.

## G10 — Changelog implementation history obscures the final release state

- Date: 2026-10-19. Author: GPT-6-Astra. Status: open release-reading friction.
- Evidence: [public changelog](https://www.sakuralang.com/changelog.md), [changes](https://www.sakuralang.com/docs/changes).
- Observation: the alpha.1 section explicitly retains earlier development history,
  including a line that LIR emission emits no WASM yet, alongside later WASM-host
  fixes in the same release section. The 0.1.0 section similarly contains an earlier
  manifest entry saying WASM drops no checks and a later entry saying it does.
  These are chronological records, not necessarily contradictions in shipped code,
  but the reader must reconstruct which statement supersedes which.
- Impact: release-notes-only migration research needs a clear final capability
  summary; copying an earlier implementation-stage sentence can produce the wrong
  baseline for the Node library or numeric kernel.
- Requested docs: preserve the history while adding final-state summaries and
  explicit supersession notes for each affected release/feature.
- Brief handling: no historical “not yet” sentence is treated as execution evidence;
  actual installed-release results must be recorded by the implementation author.

## G11 — Changelog Markdown retains repository-relative links that return 404

- Date: 2026-10-19. Author: GPT-6-Astra. Status: confirmed public-link failures.
- Evidence: [changelog export](https://www.sakuralang.com/changelog.md).
- Observation: its relative links resolve on the website to
  `https://www.sakuralang.com/STABILITY.md`,
  `https://www.sakuralang.com/docs/rfcs/0001-stability-guarantees.md` and
  `https://www.sakuralang.com/website/content/docs/install.mdx`.
  Read-only browser fetches returned HTTP 404 for all three on the research date.
- Impact: the public notes' policy, RFC and platform references interrupt the
  website-only reading path needed by fresh authors and later migration research.
- Requested docs: rewrite exported links to the corresponding public website pages
  where available, and otherwise explicitly identify an external reference.
- Brief handling: sources link directly to functioning public install, changes and
  capabilities pages. The broken destinations were not replaced with repository reads.

## G12 — The memory tour both announces and defers W8001

- Date: 2026-10-19. Author: GPT-6-Astra. Status: open current-page inconsistency.
- Evidence: [memory tour](https://www.sakuralang.com/docs/tour/memory).
- Observation: the reference-counting discussion says the compiler emits W8001
  when a retained older collection forces copying in a loop. The costs section
  says W8001 copy diagnostics are planned with Perceus reuse after 1.0.
- Impact: authors using persistent collections cannot tell whether an absent
  warning establishes anything about copying in their installed release.
- Requested docs: state current diagnostic availability by backend/release and
  separate implemented value semantics from future optimisation diagnostics.
- Brief handling: collection behaviour is tested by results; none of the briefs
  requires a warning or claims allocation/performance evidence from its absence.

## G13 — Resource affine-use wording is stronger than the capability guide

- Date: 2026-10-19. Author: GPT-6-Astra. Status: open guarantee-wording inconsistency.
- Evidence: [memory resources](https://www.sakuralang.com/docs/tour/memory#resources), [scoped escape rules](https://www.sakuralang.com/docs/tour/capabilities#scoped-escape-rules).
- Observation: the memory guide calls resources affine, used at most once as a
  whole; the capability guide explicitly says v0 does not check affine use counts
  and runtime operations reject access after closure. A reader can mistake the
  first statement for a static use-count guarantee.
- Impact: code using chunked files, HTTP bodies and fibers needs to know what the
  compiler actually rejects versus what is a scope rule or runtime restriction.
- Requested docs: reconcile affine terminology with implemented scoped-escape and
  runtime checks, using a release-specific example of the distinction.
- Brief handling: resource work stays inside documented callbacks/scopes; no brief
  treats “affine” as proof that all possible double-use mistakes are statically rejected.

## Verification of this research deliverable

The six briefs are Markdown requirements, not executable language examples. Each
contains purpose, behaviour, I/O, bounds, std surface, permissions, more than 20
separately named prose acceptance scenarios, non-goals and public sources. Their
eventual program size, tests, cassette and installed-release gates are requirements,
not measured results. Public cited page exports were fetched successfully; G11's
three intentionally reported failures remain failed.

No compiler or website build checks, Sakura suites, solver runs, Node embedding
checks, install smokes or CI runs were performed in this research pass. None is
reported as passing. The eventual implementation authors own those gates and must
report exact results and limitations. This pass changes documentation only.
