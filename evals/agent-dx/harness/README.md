# Harness

Python 3.11+ standard library only, plus Git, Node and a built `sakura`.

- `offline.py` validates task metadata and executes trusted, checked-in gold and
  negative fixtures (invoked by the Cargo test `agent_dx::offline_task_oracles`).
- `runner.py` runs agent episodes, replays their transcripts and judges episodes run
  outside it (`judge`).
- `test_runner.py` tests the runner offline (Cargo test `agent_dx::runner_offline`,
  so `cargo test --workspace` in `ci.yml` runs it on Linux and macOS without secrets).
- `fixtures/` holds a committed canned transcript that CI replays.

## Offline fixture validation

`offline.py --sakura /absolute/path/to/sakura` runs independent tasks in a thread
pool. `--jobs N` selects a positive worker count. The default is the CPU count
(or 1 if unavailable), capped at 32; at most the selected task count runs at once.
`--jobs 1` runs serially. Each task retains its own directory and Git repository;
gold, hollow and controls within a task run sequentially. Report lines print in
task order, so successful serial and parallel reports are byte-identical.
`--task 03-path-label` selects one task.

Run the scheduling regression checks without a compiler or provider calls:

```sh
python3 -m unittest -v evals/agent-dx/harness/test_offline.py
```

## What an episode does

1. Copies only the task's `starter/` into a fresh directory under ignored
   `target/agent-dx/`. Oracles, gold, hollow and the repository are never copied in.
2. Gives the model the task prompt, a neutral system prompt and five tools:
   `list_files`, `read_file`, `write_file` (paths must stay inside the project and
   out of `.git`), `sakura` (allowlisted subcommands: check, test, fmt, fix, explain,
   prompt, doc, audit, verify, help; never `--record`), and in the `skill` condition
   `load_skill`, which serves the skill embedded in this exact binary (as
   `sakura setup` writes it). The system prompt lists the skill by name and
   description only, so loading it is the model's choice; `skill_loaded` records it.
   `sakura` runs with a filtered environment (no provider key), the task's 30-second
   tool timeout, and with `--sandbox auto` (the default) under a network-denying
   wrapper: `sandbox-exec` on macOS, `unshare --user --net` on Linux.
3. Loops until the model answers without tool calls or a cap is hit: 40 requests,
   the task's aggregate input/output tokens (input counted before each request,
   `max_tokens` set to the remaining output allowance), wall seconds (model latency
   plus tool time) and repair cycles. Termination is one of `end_turn`, `max_turns`,
   `input_cap`, `output_cap`, `wall_clock`, `repair_cap`, `budget`,
   `script_exhausted`, or the provider's stop reason.
4. Snapshots a submission before the first `check`/`test` after the files changed,
   or at the final answer; at most 1 + 4 repair submissions. Each snapshot is
   judged afterwards in a separate verifier directory with its own baseline commit:
   the file guard, `check --frozen-specs all --since <baseline>`, no holes or
   diagnostics, the visible tests and every oracle separately (`offline.py`'s rules).
5. Writes `<task>.<condition>.<model>.in<input cap>.jsonl` (start, count, request,
   response, tool, submission, verdict, end events) and `summary.json` with per-episode
   metrics (pass@1, success, iterations to green, censoring, diagnostics, tokens, cost,
   documents, input budget, skill activation, unauthorized edits) and per
   model/condition/input-cap counts with 95% Wilson intervals, cost per accepted task and
   `input_cap` stops.

## Retrieval and budget telemetry (harness 2, design §17.165 (3))

- `documents`: one record per lookup, in order, failed ones included: `id`
  (`skill:SKILL.md`, `explain V3001`, `prompt --budget 3000`, `doc List.map`, `help`),
  `ok` (the tool succeeded, `sakura` exited 0, and `doc` did not answer
  `no symbol matches`), `bytes` the model received, `approx_tokens` (bytes / 4, an
  estimate, not a tokenizer count), `truncated` (cut at 8,000 characters), `turn`, and
  `input_used`, the aggregate input billed so far including the request that asked.
- `diagnostics.reported`: codes of diagnostics that `check`, `test`, `fix`, `audit` or
  `verify` reported (`--json` diagnostic records, or `error[…]`/`warning[…]` headers);
  `diagnostics.quoted`: codes that appear in retrieved text (`explain`, `prompt`, `doc`,
  `help`, skill files). `seen` keeps harness 1's meaning (any code in `sakura` output).
- `input_budget`: `cap`, `used`, `remaining`, and `refused_request`, the counted input
  of the request the cap refused (null if the cap did not stop the episode).
- `input_cap`: the episode's arm. `--input-caps 40000,60000` runs every cell once per
  cap, shuffled together by `--seed`, so the arms are paired in one design; each arm's
  maximum cost is planned and reserved from its own cap. Caps are at most 60,000.

Replay refuses transcripts recorded by another harness version.

## Backends

- `scripted`: responses from a JSON file `{"responses": [...]}` in Messages API shape
  (an optional `elapsed_s` stands in for latency). One episode per run.
- `replay`: re-drives an episode from a transcript's recorded responses and token
  counts, re-runs every tool and oracle, recomputes accounting, and exits 1 if any
  metric differs from the recorded one. No network.
- `live`: the Anthropic Messages API over `urllib`, key read only from
  `ANTHROPIC_API_KEY`, no SDK and no retries. It refuses to start without
  `--budget-usd` in (0, 10], `--approved-by icantcodefyi`, explicitly pinned
  `--model ID=IN/OUT` prices no higher than $4/$20 per million tokens, an existing
  ledger, and (for a non-loopback API) the network sandbox. Before each episode it
  stops if that episode's maximum cost exceeds what remains of the run budget or of
  the $10 owner total; before each request it reserves `counted input × input price
  + max_tokens × output price` in `reports/ledger.json` and settles the reported
  usage afterwards. A failed request keeps its reservation as spent and halts the
  run. `--max-episodes` (default 48) caps the episodes one approved run may plan;
  across runs the dollar ledger is the only cap (§17.182 (4)). Caching, thinking,
  server tools and sampling overrides are never requested.

Tests exercise live mode only against a local mock server.

## Owner commands

From the repository root, after `cargo build --release -p sakura_cli --locked` in
`compiler/`. First review the plan and cost bound (no key, no call):

```sh
python3 evals/agent-dx/harness/runner.py run --backend live --plan-only \
  --sakura compiler/target/release/sakura \
  --model claude-opus-5-5=4/20 --model claude-sonnet-5-5=2/10 \
  --budget-usd 10 --approved-by icantcodefyi
```

Then the paid pilot, with the key in the environment (recheck model IDs and prices
first; the prices are the README's checked 2026-10-18 values):

```sh
ANTHROPIC_API_KEY=… python3 evals/agent-dx/harness/runner.py run --backend live \
  --sakura compiler/target/release/sakura \
  --model claude-opus-5-5=4/20 --model claude-sonnet-5-5=2/10 \
  --budget-usd 10 --approved-by icantcodefyi --seed 1
git commit -m "chore(evals): record agent-eval spend" -- evals/agent-dx/reports/ledger.json
```

The next run (§17.182 (4)) pairs the arms on tasks 03, 04 and 10: 24 episodes per seed,
at most $5.04 per seed. Seed 1 needs a fresh key and the owner's written approval; seed 2
needs the owner to raise the $10 total. Check the plan first:

```sh
python3 evals/agent-dx/harness/runner.py run --backend live --plan-only \
  --sakura compiler/target/release/sakura --input-caps 40000,60000 \
  --tasks 03-path-label,04-local-env-handler,10-http-validation \
  --model claude-opus-5-5=4/20 --model claude-sonnet-5-5=2/10 \
  --budget-usd <approved> --approved-by icantcodefyi --seed 1 --max-episodes 24
```

Transcripts and the summary land in `target/agent-dx/runs/<run-id>/` (ignored;
raw messages stay private until redacted). Replay one with
`runner.py replay --sakura … --transcript <file>`. Alternatively dispatch
`agent-eval.yml` from GitHub after creating the `agent-eval` environment with the
owner as required reviewer and its `ANTHROPIC_API_KEY` secret; it commits the ledger
back to `main` and uploads only the summary.

## Subscription episodes (`judge`, design §17.301 (3c) (iii))

Episodes run through a coding-agent seat (Claude Code, Codex) instead of the Messages
API spend nothing under the cap; the runner only judges them. For each task, give a
delegated agent a fresh copy of the task's `starter/` and its `prompt.md` and nothing
else from the repository (no gold, hollow or oracle), with the network denied to
`sakura` and only the tools of step 2 above (read, write, the allowlisted `sakura`
subcommands, the skill in the `skill` condition). Then lay the results out as

```
<episodes>/<task-id>/submission/       the project's files as the agent left them
<episodes>/<task-id>/transcript.jsonl  the client's session transcript
```

and run, with no key and no network:

```sh
python3 evals/agent-dx/harness/runner.py judge --sakura compiler/target/release/sakura \
  --episodes <episodes>
```

`submission/` must hold exactly the starter's inventory (`sakura.toml`, `src/main.sak`;
`target/` is ignored): a project the agent ran `sakura setup` in carries `.claude/` or
`.agents/` and fails the guard, so copy those two files out. Each submission gets
`verdict()`, the live runner's acceptance (the guard, `check --frozen-specs all
--since`, no holes or diagnostics, the visible tests, every oracle). From the transcript
it reads the model family (`claude` or `openai`; anything else, or two families, is
refused), the condition (`skill` when the transcript lists the `sakura` skill or names
its `SKILL.md`, else `baseline`) and `skill_loaded` (a tool call invoked the skill or
read its `SKILL.md`). It prints one record per task and writes `judged.json` with the
per-family/condition accepted counts and 95% Wilson intervals. Only the final state is
judged, so there is no pass@1 or iterations-to-green, and the seat's tokens and cost are
not measured. Label the numbers "subscription episodes, <family> family, judged
offline".

## Limits

The runner is narrower than the README protocol in these ways: one worker (not two),
so in-flight reservations are trivially serial; no memory or process-count limit on
generated programs; filesystem isolation is the tool path checks plus a disposable
directory, not a container, and `sakura` itself can read the host. Run live
episodes on a disposable machine or the workflow runner. The ledger is a committed
file, so concurrent runs from two checkouts are not coordinated.
