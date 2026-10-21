# Agent DX pilot: authored tasks and offline acceptance

Status: **first paid pilot run; report available, owner review pending**.
See the [2026-10-18 report](reports/2026-10-18-pilot.md): 48 episodes, $4.200730
spent. The next run (design §17.182 (4)) is 24 paired episodes per seed under the
remaining dollars; it needs a fresh key and the owner's written approval.
This implements the 12-task authoring portion of [DX5](../../docs/plans/agent-dx.md),
[design §17.146](../../docs/design.md) and the evaluation design in
[research/11 §5](../../research/11-agent-dx.md). Research §4 covers client setup;
the evaluation section is §5 in the current file. This suite is separate from the
compiler's Core evaluator, `compiler/tools/sakura_eval`.

The owner decision in the plan's §3 overrides the original cross-provider proposal:
**Anthropic models only, at most $10 total on the owner's Anthropic key**. Task
creation and offline validation do not authorize a paid run. The episode runner, its
spend ledger and the owner-dispatched workflow are described in
[harness/README.md](harness/README.md). The owner-approved run is recorded in the
report and ledger.

## Tasks and acceptance

Each task contains `prompt.md`, `starter/`, `gold/`, `hollow/`, `oracle/`, and
`task.json`. Gold and hollow are complete replacement project trees for offline
validation. All use the JS backend. Inputs are deliberately small; prompts are at
most 2,400 UTF-8 bytes and starting sources at most 80 lines, enforced in CI.
There are two tasks per category, with 25 independently executed oracle cases.
Each case can contain several related behavioral assertions.

| ID | Category | Change | Skills | Difficulty | Oracle cases |
|---|---|---|---|---|---|
| 01-bounded-clamp | Syntax/data | Inclusive clamp under ordered bounds | Branches, contracts | Easy | Outside bounds; interior/equal bounds |
| 02-positive-summary | Syntax/data | Positive readings → doubled count/total record | Filter/map, records | Easy | Mixed/duplicate values; empty/nonpositive |
| 03-path-label | Diagnose/repair | Fix Path import and String return | Diagnostics, normalization, UTF-8 bytes | Easy | Redundant path segments; non-ASCII |
| 04-local-env-handler | Diagnose/repair | Run an effectful callback under local Env and a pure signature | Effect diagnostics, handlers | Medium | Configured keys; absent keys |
| 05-stable-unique | Holes/retrieval | Fill order-preserving deduplication hole | Std lookup, holes | Easy | First-occurrence order; empty/negative |
| 06-typed-recovery | Holes/retrieval | Recover BadCount with the supplied fallback | Typed errors, catch, holes | Medium | Signed integers; invalid/empty |
| 07-health-route | Application | Add GET /health, preserve /version | Router dispatch, methods | Medium | Health response; version/404/405 |
| 08-cli-prefix | Application | Add optional prefix before transformed name | Cli schema, Option, strings | Medium | Prefix/uppercase/default; invalid arguments |
| 09-scripted-reading | Deterministic effects | Fix JSON shape in a scripted LLM test | Schema, Llm.scripted | Medium | Positive readings; zero/negative |
| 10-http-validation | Deterministic effects | Decode a bounded count with HTTP recovery | Fake HTTP, refinements | Medium | Valid values; malformed/refinement/size failures; failed exchanges |
| 11-successor-contract | Assurance | Satisfy the frozen successor postcondition | Requires/ensures, frozen specs | Easy | Lower/upper edges and invalid callers |
| 12-permitted-mode | Assurance | Read APP_MODE using the existing grant | Env attenuation, permissions | Medium | Granted key; absent/invalid/denied key |

Task 03 initially emits the code set `V1002`, `V2001`; task 04 emits `V3001`.
Tasks 01, 05 and 06 initially have one typed hole, with no diagnostic code. Other
starters check clean but have incomplete behavior. Task 09's visible test fails on
schema decoding; task 11's fails on its postcondition. CI asserts the exact code
sets and hole counts separately. Successful `check` is never acceptance by itself.

Tasks 01, 02, 03, 07, 08 and 10 are the six **translation-eligible** portable tasks.
TS/Python translations, matched library availability and runtime pins are not
implemented or funded here. Effects/holes/assurance are a separate Sakura score.
These are original evaluation variants, not copies of the cookbook solutions.
Because the corpus is committed publicly, “hidden” means hidden from the episode's
agent filesystem and feedback; it is not a claim of secrecy or no contamination.

## Internal manifest v1

[`task.schema.json`](task.schema.json) is the complete internal schema, not a
stable public API. Unknown fields fail validation. `harness/offline.py` validates
its explicitly supported JSON Schema subset using Python's standard library and
fails if that schema grows unsupported keywords. Paths cannot escape the task.

- `schema_version`, `id`, `revision`: schema version 1, directory-matching task ID,
  and task revision. Increment revision when prompt, input contract or oracle changes.
- `category`, `skills`, `difficulty`: one of six categories, exercised skills, and
  easy/medium/hard author estimates; difficulty is not a measured model result.
- `compiler`: the source revision used during authoring and backend `js`. The actual
  tested compiler commit, version, toolchain, Node and bundle hashes also belong in
  every run record; the recorded baseline does not claim forward compatibility.
- `starter_files`: SHA-256 of each project file. `starter_sha256`: SHA-256 of the
  concatenation, sorted by relative POSIX path, of `path + NUL + hex_digest + LF`.
  The inventory must match exactly; hashes cover raw bytes, including the manifest.
- `editable_files`: only `src/main.sak`; other files, including permissions,
  are immutable. Existing types, signatures, contracts and visible tests are frozen.
- `frozen_baseline`: `commit-starter`, ref `refs/heads/eval-baseline`, specs `all`.
  This is a creation instruction, not a placeholder SHA: commit the exact starter
  in the disposable project and record its resulting 40-digit commit externally.
  Pass that immutable SHA to `--frozen-specs all --since <sha>`. The agent's own Git
  refs are never the authority for final verification.
- `oracle`: enumerated source files and the fixed verifier command
  `sakura test --json --filter oracle/`. Each file is appended separately to a copy
  of the accepted candidate in a fresh verifier context. The compiler check and
  frozen-spec checks run **before** appending oracle tests. Each invocation must
  execute exactly its one named oracle; the visible example is deliberately filtered
  out, so its failure cannot masquerade as every hidden test failing.
- `expected_diagnostics`, `expected_holes`: initial distinct diagnostic code set
  and exact hole count, checked against JSON schema version `1.0` records.
- `budget`: aggregate estimated/max input and output tokens, repair cycles, wall
  seconds and per-tool seconds, as specified below. These are runner requirements;
  the offline validator enforces only its 30-second tool timeout.
- `comparison`: portable eligibility, proposed languages and the reason. It does
  not assert translation equivalence before translation review.

## Offline checks and CI

From `compiler/`, with the pinned Rust toolchain, Node 26, Python 3.11+ and Git:

```sh
cargo test -p sakura_cli --test agent_dx --locked -- --nocapture
cargo test --workspace --locked
```

The integration test invokes the offline validator with Cargo's freshly built
`sakura` binary. Existing `ci.yml` already runs the workspace tests on macOS and
Linux; no workflow change or secrets are needed. For one authoring iteration:

```sh
python3 evals/agent-dx/harness/offline.py --sakura /absolute/path/to/sakura --task 03-path-label
```

The full check enforces all 12 manifests, category balance, six portable tasks,
file inventories/hashes, compact source sizes and exact initial diagnostics. It
checks gold and hollow candidates under the frozen starter, then compiles and
executes each oracle separately: **gold passes all 25; hollow fails all 25**.
Hollow programs must compile without holes, so a syntax error cannot count as a
successful negative control. An oracle imports every name it uses; a same-meaning
re-import is silent. Gold also passes the unchanged visible tests. Task 11's hollow triggers the expected
postcondition defect `D021`; other hollow cases must produce failed test records,
not an unrelated defect or process failure.

For each task, additional negative controls must reject changed permissions,
a trust shortcut, a removed visible test and a newly added hole. Dedicated controls
also reject weakening task 11's postcondition and widening task 04's effect row.
These distinguish behavioral failure from structural rejection. A permission
change is rejected by an external byte comparison; tests/contracts/rows are
checked using the compiler's frozen-spec gate. Fixture hashes, JSON records and
process exit statuses are all checked; no oracle result is inferred from a bare
nonzero exit. Schema controls reject unknown fields and path traversal.

This validator runs trusted checked-in fixtures. Its syntactic trust/host checks
are a task policy check, **not an adversarial sandbox**. It creates scratch projects
under ignored `target/agent-dx/`, removes them on exit, filters inherited environment
variables and kills a timed-out process group. Do not run untrusted model code
through it on the host. No build, compiler or diagnostic semantics change here.

## Episode protocol

1. Freeze the task revision/hashes and randomize paired task order before comparing
   two pinned Anthropic model IDs. Record exact provider IDs/revisions, reasoning,
   sampling, tokenizer, harness/client/compiler/toolchain versions and date. Use
   fresh independent context per task × condition × model; no retries as new samples.
2. The initial matrix is **12 tasks × 2 conditions × 2 models × 1 episode = 48**.
   Baseline gets prompt/starter, compiler/help and normal CLI docs lookup, but no
   installed skill. Skill-discovery gets the same resources plus the matched
   discoverable skill. Neutral project instructions and tool/document budgets are
   otherwise identical. Record whether discovery and loading actually happened.
   Forced-load, guide-only, bootstrap and MCP comparisons are later separate
   conditions, not silently mixed into the skill score. MCP must get the same
   information budget as CLI. No extra conditions are funded automatically.
3. Export **only** prompt and starter into a disposable isolated filesystem; do not
   mount this repository, gold/hollow/oracles, sibling worktrees, provider secrets
   or the verifier's Git store. The API broker owns the key outside the agent
   environment. Deny generated programs network access and bound processes, memory,
   output and wall time. Commit the starter, retain its SHA and full hashes outside
   the sandbox, and preserve that independent baseline even if the agent edits Git.
4. The initial submitted patch is the pass@1 candidate. Snapshot it before showing
   its compiler/test feedback. Submission is the first check/test after an edit,
   or the final answer if there is no such call; take the snapshot before executing
   that tool. Multiple edits before a check form one candidate. Read-only checks
   before any edit describe the starter, not a submission. Permit at most four
   further edit→check/test repair
   cycles within the same episode and aggregate token/time cap. Hidden tests run
   externally on snapshots; never return their code, inputs or expected outputs.
   Only public compiler/visible-test feedback drives repair. Final acceptance
   requires frozen declarations/tests/authority, no holes or new diagnostics,
   unchanged visible tests passing, and every independent oracle passing.
5. Record initial/final diagnostic code sets and holes, edits, compiler/test calls,
   iterations, retrieved document IDs, activation, tokens including reasoning and
   cached categories, latency, cost, termination and unauthorized edits. Persist
   failed and capped episodes too. Keep raw provider messages private until redacted.

[harness/README.md](harness/README.md) maps this protocol onto `runner.py`, lists
where the runner is narrower than this protocol (one worker, no memory limit), and
gives the owner's commands. DX5 is complete only after the owner-approved pilot and
its reviewed report.

## Metrics and reporting

- **One-shot pass@1:** accepted initial patches / all fresh episodes in a cell;
  missing submissions/timeouts fail. **Budgeted success:** accepted final candidates
  / all episodes. Repairs never increase k. There is one independent sample per
  task/model/condition here, so no pass@3 claim is possible.
- **Iterations-to-green:** number of submitted edit→check/test cycles to the first
  independently acceptable snapshot (initial patch is cycle 1). Report success
  fraction and conditional median/p90 together. Exhaustion/timeouts are censored
  at the performed cycles, never counted as zero. Count every tool call separately.
- **Diagnostic-fix success:** for tasks 03/04, episodes where all expected initial
  codes disappear, no new diagnostics/holes appear, all behavior tests pass, and
  specs/authority remain intact, divided by all attempted seeded-diagnostic
  episodes. Report recurrence and new codes separately. Runtime failures in 09/11
  are separate outcomes, not silently added to the compiler-diagnostic denominator.
- **Cost per accepted task:** all billed episode cost, including failures, divided
  by accepted episodes; undefined at zero acceptance. Also report tokens per
  acceptance, time to first green, activation/retrieval failures, unauthorized
  edits and a separately identified sample of human review time.

Report per-task outcomes and every failed episode, plus raw numerators and
denominators per condition/model. Include 95% Wilson intervals for success
fractions and paired task differences, with uncertainty resampling tasks rather
than treating repair turns as independent observations. Twelve tasks constitute
a diagnostic pilot, not a leaderboard or evidence of language superiority.

## Token sizing, dollars and approval

Each task budgets **12,000 aggregate input + 2,000 aggregate output tokens** as an
unmeasured planning estimate for a short implementation and diagnostic follow-up.
One initial context of roughly 6,000 tokens (prompt, compact starter, tool schemas,
selected docs/skill) plus roughly 6,000 across follow-up requests gives 12,000;
2,000 output tokens covers a small patch and reasoning. Repeated history, tool
results, system instructions and reasoning are charged to these totals. Selective
lookup is essential: do not inject the whole std reference into every request.
Actual tokenizer counts must be estimated before scheduling each episode.

The **hard per-episode ceilings** are 40,000 aggregate input, 4,000 aggregate output
(including reasoning), four repair cycles, ten minutes, and 30 seconds per tool.
At most two workers. These tighten §17.146's initial 60,000/12,000/eight-cycle caps
to respect the owner's smaller budget; they do not promise every task will finish.
Every task has the same caps, even across conditions, to avoid favoring a model.

Use two admissible price envelopes: model A at **at most $4/million input and
$20/million output**, model B at **at most $2/million input and $10/million output**.
Each gets 24 episodes with identical token caps. Disable caching, fast mode and
paid server tools. These are accounting ceilings, not permission to select a
model or assume future availability. The [official Anthropic pricing page](https://platform.claude.com/docs/en/about-claude/pricing),
checked 2026-10-18, lists Opus 5.5 at $4/$20 and Sonnet 5.5 at $2/$10, so an
Opus-class/Sonnet-class pair currently fits this envelope. Exact provider IDs remain
for the owner-approved run; recheck availability, prices and billing modifiers then.
This retains an Opus-class comparison role while replacing the original plan's
GPT-family model with an Anthropic model under the owner's restriction.

```text
Estimated A episode = (12,000 × $4 + 2,000 × $20) / 1,000,000 = $0.088
Estimated B episode = (12,000 × $2 + 2,000 × $10) / 1,000,000 = $0.044
Estimated pilot     = 24 × ($0.088 + $0.044) = $3.168

Maximum A episode   = (40,000 × $4 + 4,000 × $20) / 1,000,000 = $0.24
Maximum B episode   = (40,000 × $2 + 4,000 × $10) / 1,000,000 = $0.12
Maximum pilot       = 24 × ($0.24 + $0.12) = $8.64
Reserve             = $10.00 - $8.64 = $1.36
```

The $1.36 is contingency, **not** permission for extra samples or escalation. The
broker must reserve each request's maximum billable cost before sending it,
account for both in-flight workers atomically, settle actual provider usage, and
stop scheduling before another request/episode would exceed the remaining cap.
All failed calls and any applicable billing modifiers count. If usage is unknown,
retain the reservation and stop; never assume a failed request was free. Disable
automatic SDK retries. A changed model, price, caching policy or token cap requires
a new estimate and owner approval; never substitute an expensive model silently.

When the runner is ready, show the owner the immutable task/bundle/compiler hashes,
two exact Anthropic model IDs, both conditions, full cost bound and offline replay
results. The owner supplies the key through `request_secret`, then explicitly
approves that concrete run with `--budget-usd 10 --approved-by icantcodefyi` (or a
smaller cap). The future workflow must be `workflow_dispatch` only, in a protected
environment with @icantcodefyi as required reviewer, and refuse missing approval or
budget. A CLI string alone is not proof of authorization: the broker/workflow
must bind approval to that run. No scheduled paid runs, non-Anthropic calls,
replication, TS/Python model runs or client UI runs are authorized by this pilot.

AI contribution: GPT-6-Astra authored the tasks, protocol and offline validation.
The human contributor owns their review and acceptance.
