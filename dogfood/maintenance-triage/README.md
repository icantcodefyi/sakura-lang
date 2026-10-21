# Maintenance Triage

A bounded assistant that turns a workshop volunteer's malfunction report into a
short, cited triage note from a small local equipment manual. Written in Sakura
Lang against the installed `sakura 0.1.0 (2014e821b87a0d30cb8061b6158f99723f0f676d)`
by Claude Opus 5.5 in a fresh session with no repository checkout, and upgraded
to `sakura 1.0.0-beta.1 (7aee189efa2dd571a8a7988a91b69f507fc46399)` (MIGRATION.md).
See BRIEF.md for the requirements and FRICTION.md for problems met on the way.

## Running

```sh
sakura run -- manual.json incident-drill.txt                                   # live: needs ANTHROPIC_API_KEY
sakura run -- manual.json incident-drill.txt --replay drill-replay.jsonl       # offline replay
```

File names are relative to `./data`. Success prints one JSON record on stdout
and exits 0. A failure prints one `triage failed: ...` line on stderr and exits
2 (usage or input), 3 (configuration: missing key), 4 (model, provider, budget
or tool failure) or 5 (the answer failed the citation checks).

Since 1.0.0-beta.1 `Llm.anthropic` no longer reads `ANTHROPIC_BASE_URL`, so the
variable may stay set (FRICTION.md).

## Authority

`sakura.toml` grants reads under `./data`, `net` to `api.anthropic.com:443`
and `env` to `ANTHROPIC_API_KEY`; nothing else. `main` narrows again with
`Fs.rooted(./data)`, `Http.scoped([api.anthropic.com:443])` and
`Env.only([ANTHROPIC_API_KEY])`. The key is read with `Env.secret` and passed
as a `Secret` to `Llm.anthropic`; it is never exposed or logged. The model gets
two read-only tools, `search_manual` and `fetch_entry`, both served by the
in-program `Manual` effect. It cannot operate equipment, run commands, write
files or reach another host.

## Bounds

Manual at most 65,536 bytes and 50 entries (IDs 1–32 ASCII letters, digits and
hyphens, unique; titles 1–80 bytes; text 1–1,000 bytes). Incident at most 8,192
bytes, non-blank. Search returns at most five hits in ID order. The agent runs
at most three model turns, with no tools on the last. The answer has a 1–500
byte summary, at most five citations and a `DocumentedStep` or
`EscalateToSupervisor` disposition.

Live local accounting limits are 20,000 tokens, $0.25 and 4 call reservations.
They were sized against the fixtures: the two-turn cassette needs 3 call
reservations (the agent's per-turn reservation overlaps the provider's), more
than 2,500 tokens (it passes at 3,000) and more than $0.02 (it passes at $0.05);
the three-turn cassette needs 4 call reservations. These are local accounting
limits computed from the adapter's request estimates, not a provider bill or a
billing guarantee.

## Handler ordering

For finite-budget provider tests and `--replay`, handlers are installed in
this order, outermost first:

1. `Budget.limits(...)`
2. `Http.scripted([])`, an empty fallback, so a cassette miss fails offline
3. `Http.recording(cassette)`
4. `Llm.anthropic(Secret.of(placeholder), Model.default())`

`Llm.replay` is not used, because it installs `Budget.unlimited` internally and
could not show the finite-budget behaviour. Tool-level tests use
`Budget.unlimited()` with `Llm.scripted(...)`. The `Manual` handler is installed
inside `run_triage` for each run, so retrieved-ID state never carries over.

## What validation establishes

- The answer decodes into the `Answer` type, including its refinements (summary
  length, at most five well-formed citation IDs, a known disposition).
  Otherwise the result is `InvalidOutput`.
- At least one entry was fetched through `fetch_entry` during this run.
- Every citation names an entry in the manual and one fetched in this run.
- A hazardous incident, found by a keyword screen, always gets
  `EscalateToSupervisor`. If the model proposed something else, the next step
  is replaced and `hazard_policy_applied` is `true`.

It does not establish that the summary or next step is factually correct, safe
or faithful to the cited entries. The hazard screen is a keyword list, not a
safety certification. Tests show that the offered tool set and the provider
host are fixed. They do not show that a model always ignores instructions
embedded in the incident.

## Tests

`sakura test` and `sakura test --target wasm` run 31 tests: the 27 numbered
scenarios from the brief, three more provider-replay cases (28–30) and the
cassette regeneration test. Synthetic cassettes are described in
`tests/fixtures/README.md`.
