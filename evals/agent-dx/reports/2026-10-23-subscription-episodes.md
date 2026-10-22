# Subscription episodes, Claude family, judged offline — 2026-10-23

**12/12 tasks accepted** (95% Wilson interval 75.75–100%), skill condition, one seed,
one episode per task, Claude family only. **Second family untested**: no Codex seat
was usable for this run. These numbers come from a subscription seat and an offline
judge; they are not comparable one-to-one with the paid pilot (see "What differs").
This is gate (b) (iii) of [road-to-1.0](../../../docs/plans/road-to-1.0.md) §7, row 2b, as
[design §17.301 (3c)](../../../docs/design.md) defines it.

## Method

- **Seat and model.** Each task ran as one delegated Claude Code subagent
  (`general-purpose`) on the owner's subscription seat. Every transcript records only
  `claude-opus-5-5`. There was no API key, no Messages API call, no `--record`, and
  nothing was charged to [`ledger.json`](ledger.json).
- **Revisions.** The source commit was `087dd9475bda2aa661b4d814100563ece69a7c5b` and
  the CLI was `sakura 1.0.0-beta.5`, a release build of that commit. Harness `2`,
  Node `v26.12.0`, Python `3.11.13`, macOS 27.2 arm64. The evaluated skill is the one
  this binary embeds, as `sakura setup --client claude` writes it (six files; bundle
  SHA-256 `b412069750fe56bd6ded016c9f63edb44295de9debd3f13d6a15bb9e8ad72f6e` by
  `runner.py`'s `load_skill_bundle`). All twelve tasks were at their committed
  revisions.
- **Timing.** The twelve episodes ran concurrently, one subagent each, between
  2026-10-22T18:58:00Z and 18:59:44Z. Each took 15–52 s of wall time and 6–15 tool
  calls.
- **Episode directory.** Each episode got its own scratch directory outside the
  repository. It held a copy of the task's `starter/` (the project), its `prompt.md`,
  the skill files, and a `sakura` wrapper. The wrapper:
  - passes only `check test fmt fix explain prompt doc audit verify help` (and
    `--help`/`--version`) and refuses `--record`;
  - runs in the project with the environment reduced to `PATH` and `TMPDIR`;
  - stops each call after 30 s;
  - runs `sakura` under `sandbox-exec` with `(deny network*)`.

  Gold, hollow, oracles and the repository were not copied in.
- **Instructions to the subagent.** It was told the harness's step-2 tool set:
  - list, read and write only inside the project;
  - read the skill files only inside the episode directory;
  - use Bash only for the wrapper;
  - no web, no subagents, no `Skill` tool;
  - at most 40 tool calls and about 10 minutes.

  As in the runner, the skill was listed by name and description and the subagent
  chose whether to load it. The task statement was given as a file to read first (the
  runner sends it as the first message). The text is the same.
- **Judging.** `runner.py judge` ran with no key and no network over
  `<task>/submission/` (the two starter files as the agent left them) and
  `<task>/transcript.jsonl` (the subagent's Claude Code transcript). Acceptance is
  the live runner's `verdict()`:
  - the file guard;
  - `check --frozen-specs all --since <baseline>`;
  - no holes or diagnostics;
  - the visible tests;
  - every oracle, each in its own verifier directory.

  The judge's report is [`judged.json`](judged.json).
- **Negative control.** Before trusting the 12/12, the judge was run on the unchanged
  starters of tasks 01, 03 and 10 with the same transcripts. All three were rejected,
  each for the starter's own defect:
  - 01: one hole;
  - 03: `V1002`;
  - 10: the visible test fails.

  Every oracle was false.

## Per-task verdicts

One episode per task, so each per-task interval is the n = 1 Wilson interval:
[20.65%, 100%] when accepted.

| Task | Verdict | Oracles | Wilson 95% | Skill loaded | Tool calls | Pilot (i) acceptances, OB/OS/SB/SS |
| --- | --- | --- | --- | --- | --- | --- |
| 01-bounded-clamp | accepted | 2/2 | 20.65–100% | yes | 6 | 1/1/1/1 |
| 02-positive-summary | accepted | 2/2 | 20.65–100% | yes | 8 | 1/1/1/1 |
| 03-path-label | accepted | 2/2 | 20.65–100% | yes | 9 | 0/1/0/1 |
| 04-local-env-handler | accepted | 2/2 | 20.65–100% | yes | 11 | 0/1/0/0 |
| 05-stable-unique | accepted | 2/2 | 20.65–100% | yes | 8 | 1/1/1/1 |
| 06-typed-recovery | accepted | 2/2 | 20.65–100% | yes | 9 | 0/1/0/1 |
| 07-health-route | accepted | 2/2 | 20.65–100% | yes | 11 | 1/1/1/1 |
| 08-cli-prefix | accepted | 2/2 | 20.65–100% | yes | 8 | 1/1/1/0 |
| 09-scripted-reading | accepted | 2/2 | 20.65–100% | yes | 9 | 1/1/1/1 |
| 10-http-validation | accepted | 3/3 | 20.65–100% | yes | 15 | 0/0/0/0 |
| 11-successor-contract | accepted | 2/2 | 20.65–100% | yes | 7 | 1/1/1/1 |
| 12-permitted-mode | accepted | 2/2 | 20.65–100% | yes | 9 | 1/1/1/1 |

| Family / condition | Accepted | Wilson 95% | Skill not loaded |
| --- | --- | --- | --- |
| claude / skill | 12/12 | 75.75–100% | 0 |
| openai / skill | untested | — | — |

The pilot columns are the [2026-10-18 pilot](2026-10-18-pilot.md)'s final acceptances
(Opus/Sonnet × baseline/skill), measured with an older skill and the alpha compiler.

**§17.301 (3c)'s rule.** No task may fail in both (iii) and (i). No task failed here,
so none fails in both. Task 10 failed all four pilot episodes and passed here. Since
no task failed, no fix was owed under §17.301 (3c), and none was made.

## Sandbox compliance

An audit of every tool call in the twelve transcripts:

- Every `Read`, `Write`, `Edit` and `Glob` stayed inside its own episode directory.
- No Bash command named a path outside its episode or ran a program other than the
  wrapper.
- No other tools were used.

Four episodes broke the letter of the rules with shell plumbing around the wrapper:

- 03 and 07: `cd <own project> && …/bin/sakura`;
- 08: `cd <own project> && ../bin/sakura … | head`;
- 10: `B=…/bin/sakura; $B …`.

07's subagent reported its own `cd`. None of these reached outside the episode.

Five agents added their own tests to `src/main.sak` (04, 06, 07, 08, 10). That is
allowed: visible tests and declarations stayed frozen, which the frozen-spec check
confirms. Several ran `sakura fmt`, which reflowed the visible test. That passed the
frozen-spec gate.

## Finding: a wrong machine-applicable `V1002` fix (task 03)

The episode passed despite this. For `Path.of(text)` with no import, `sakura check`
reports:

> ``cannot find `Path` in this scope; `Path` is a constructor of `Matcher` (`std/http`):
> write `Matcher.Path(…)` ``

with a `MachineApplicable` fix ``Add `import std/http.{Matcher}` `` and a `Suggested`
``Use `Matcher.Path` ``. The intended `import std/path.{Path}` is not offered. The
agent noticed and found the module with `sakura doc`. An agent that ran `sakura fix`
would have applied the wrong import.

The cause, in `sakura_hir`:

1. `declare` puts sum-type variants into module scope.
2. `compute_exports` therefore exports `Matcher`'s variant `Path` from `std/http`
   beside `std/path`'s type `Path`.
3. `std_import_fix` sees two exporters and gives up.
4. `std_ctor_hint` (§17.301 (4) (i)) then finds exactly one sum-type owner and offers
   its import.

It is recorded here, not fixed: no task failed, and the fix is a diagnostic change for
its own reviewed commit. A candidate rule: among several std exporters, take the
single one whose export is not a variant.

## What differs from the paid pilot

- **Seat, not runner.** The seat's tokens, cost and latency are not measured. Only the
  final state is judged, so there is no pass@1 or iterations-to-green. The 40-request,
  token and wall-clock caps were given as instructions, not enforced. The largest
  episode used 15 tool calls and 52 s.
- **Instructions, not tool schemas.** The filesystem rule was an instruction plus the
  audit above. The runner instead enforces it with tool path checks. As with the
  runner, there is no container, and `sakura` itself can read the host.
- **One seed, one family, one condition.** The subagent's Claude Code system context
  (which includes the repository's agent instructions) is not the runner's neutral
  system prompt. The corpus, including solutions, is public, so withheld is not
  secret.
- **Newer software.** The compiler and skill are newer than the pilot's (`1.0.0-beta.5`
  vs `0.1.0-alpha.1`), so the two runs measure different software.

Raw transcripts stay private (they are the seat's session logs). The judge's
per-episode records, including the models read from them, are in `judged.json`.

AI contribution: Claude Opus 5.5 ran the episodes through delegated subagents, judged
them, and wrote this note. The human contributor owns its review.
