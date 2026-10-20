# Maintenance Triage: a bounded assistant for a workshop's equipment manual

- Brief author: GPT-6-Astra (Codex family), 2026-10-19.
- Status: website-only brief, before implementation; no paid calls or execution evidence.
- Required domain: `Llm` agent loop with cassettes.
- Intended program author: Claude Opus 5.5 (Claude family), one fresh session with no repository checkout or path to one.
- Initial release: installed `v0.1.0-alpha.1`; actual version/build revision pending.
- Program header must record actual author, fresh-session provenance and release built against.

## Purpose and user-visible behaviour

A workshop volunteer describes a malfunction and receives a short, cited triage
note from a small local equipment manual. The assistant may search manual entries
and retrieve a named entry through two read-only tools. It returns a structured
summary, relevant entry IDs and either a documented next step or an escalation to
the supervisor. It never operates equipment, executes commands or sends messages.

Use `Agent.run` for the model/tool loop. A proposed answer must cite at least one
entry actually retrieved during that run; reject unknown or merely invented
citations. A successful answer without a retrieval is invalid. The validation
checks citation provenance and shape, not the factual truth of arbitrary model
prose. For hazardous descriptions, the application policy requires supervisor
escalation; do not present the tool as a safety certification.

Keep a small user-defined manual effect for search, fetch and retrieved-ID tracking.
Install its handler inside the program, backed by already validated manual data;
tests replace it with a deterministic handler. This gives tool calls real,
observable behaviour without granting the model filesystem access of its own.

## Inputs, outputs and bounds

CLI arguments name the local manual and incident file under `./data`. Bound each
read: manual at 65,536 bytes, incident at 8,192 bytes. The manual contains at most
50 entries, each with an ID of 1–32 ASCII letters/digits/hyphens, a title of 1–80
UTF-8 bytes and text of 1–1,000 bytes; reject duplicate IDs. Empty incidents fail
before any model call. Search returns at most five matching entries in ID order.

Treat incident and manual prose as untrusted data. Keep the system instructions
fixed and put incident text in the documented untrusted prompt part. Tool results
remain data; neither incident nor manual text may select extra tools or hosts.
The final result has a nonempty summary capped at 500 bytes, at most five citation
IDs and a small disposition variant. Print a validated JSON record or a concise
typed failure; do not print raw provider responses or credentials.

Limit the loop to three model turns, with no tools on its final turn. Install
finite token, call and local USD-accounting limits; choose and record token/cost
values against the fixture's request estimates rather than promising a provider
bill. The call limit must account for the agent's overlapping reservation with
the provider: the documented two-turn fixture needs three call reservations.

## Std surface and permissions

Use `Agent`, `Llm`, `Tool.of` with `@tool`, `Prompt`/its untrusted part, `Budget`,
`Env.secret`, `Secret`, `Http`, `Fs`, `Cli`, `Json`, refinements and typed
`LlmError`/`ToolError` handling. The manual effect's operations have explicit typed
errors, including unknown-entry failure; tool errors must remain observable.
Use handlers to keep retrieval state private to one run.

A live configuration grants reads only to `./data`, `net` only to
`api.anthropic.com:443`, and `env` only to `ANTHROPIC_API_KEY`. The key stays a
`Secret` passed to `Llm.anthropic`, with no `Secret.expose` or logging. There are
no file writes, subprocess grants or other endpoints. Tests use placeholder
secrets, deterministic manual handlers and synthetic cassettes. A live paid call
is not required or authorised by this brief.

For finite-budget cassette tests, follow the budgeted-agent recipe: outer
`Budget.limits`, an empty scripted HTTP fallback, `Http.recording`, then
`Llm.anthropic`. `Llm.replay` installs an unlimited budget internally, so it is
appropriate for provider-shape replay cases but cannot establish the finite-budget
claim. Record this handler ordering in the eventual project documentation.

## Acceptance: at least 20 separately named sakura test cases

Each numbered scenario is a separate test. Include synthetic provider cassettes
for success, refusal, malformed output and tool/error paths, with provenance and
regeneration notes. Exercise tools under `Llm.scripted` as well as provider-shaped
HTTP replay; do not substitute a canned final answer for a real tool call.

1. Search then fetch then answer produces the expected citation and summary.
2. The success case proves the actual fetch tool ran through its captured activity record.
3. A final answer citing an unknown entry is rejected.
4. A citation to an existing but never retrieved entry is rejected.
5. An answer with no retrieval is rejected even if its prose looks plausible.
6. A malformed structured answer is `InvalidOutput`, without a success report.
7. An empty or over-500-byte summary is rejected by the output boundary.
8. More than five citations is rejected rather than silently truncated.
9. Search returns at most five matches in ID order for the same query.
10. Fetching an unknown ID becomes a `ToolError` and produces no successful-fetch record.
11. Model arguments with an invalid ID shape fail before the manual tool executes.
12. An unoffered tool name cannot execute a tool or grant new authority.
13. A model asking for another tool on the third and final turn fails with `InvalidOutput`.
14. A provider refusal remains the typed refusal outcome, not a fabricated triage result.
15. Truncated provider output remains distinguishable from malformed application output.
16. A cassette miss fails offline and never falls through to a live provider.
17. The documented two-turn budget case succeeds with three call reservations.
18. Reducing that case to two call reservations rejects the second provider call.
19. A token limit below the initial estimate fails before consuming any scripted response.
20. A missing live key becomes a typed configuration failure before any HTTP call; exercise this with `Env.of`.
21. Prompt construction preserves incident text as an untrusted part and cannot replace the fixed system instruction.
22. Injection-like incident text cannot add a tool or host; assert the offered tool set and recorded requests, not a claim that an LLM always ignores instructions.
23. Duplicate manual IDs are rejected before model execution.
24. An over-limit manual or incident fails the bounded-read/validation stage before model execution.
25. A fresh installation of the manual handler has no retrieved IDs from a previous run.
26. Captured output and the committed cassette contain no placeholder secret value; credentials travel only through the secret channel.
27. A hazardous-input policy case requires the escalation disposition even if the scripted model proposes an ordinary repair next step.

Document exactly what the model validation establishes. Tests do not prove that
a freely generated repair recommendation is factually correct or safe.

## Delivery and release evidence

Place the project at `dogfood/maintenance-triage/`, with at least 300 substantive
non-test `.sak` lines and at least 20 tests. Manual validation/search, tool adapters,
per-run state, answer checks, budgets and CLI reporting provide the scope. Opus
uses only installed alpha.1, its setup skill and the website. Acceptance requires
installed `v0.1.0` CI on `js` and `wasm`, with a cassette for the provider service.
Record first-migration changes against the notes and alpha.1 failures as friction.
After 1.0.0-beta.1 tags, Astra writes `MIGRATION.md` from notes alone before Opus edits;
every migration hunk must be on the list. Later Astra review edits no code.

## Non-goals and open friction

No live paid evaluation, internet search, remote manual service, autonomous repair,
shell access, durable agent memory, hard billing guarantee or prompt-injection
immunity claim. G01, G06 and G08 in [the gap register](../website-gaps.md) affect
version selection and budget composition. Do not grant more authority to make an
unaffordable or undocumented path work.

## Website evidence

Read on 2026-10-19:

- [Scripted tool recipe](https://www.sakuralang.com/docs/cookbook/scripted_tool): actual tool invocation and tool failures.
- [Budgeted agent recipe](https://www.sakuralang.com/docs/cookbook/budgeted_agent): final-turn rule, finite-budget nesting and overlapping reservations.
- [Cassettes](https://www.sakuralang.com/docs/agents/cassettes): synthetic provider fixtures and secret redaction.
- [Std reference](https://www.sakuralang.com/docs/stdlib): agent, prompt, tool, budget, secret and validation APIs.
- [Handlers](https://www.sakuralang.com/docs/tour/handlers), [capabilities](https://www.sakuralang.com/docs/tour/capabilities) and [testing](https://www.sakuralang.com/docs/tour/testing): scoped state, trust boundaries and deterministic evidence.
- [Install](https://www.sakuralang.com/docs/install), [changes](https://www.sakuralang.com/docs/changes) and [changelog](https://www.sakuralang.com/changelog.md): published baseline and compatibility caveats.

The corpus workflow follows the supplied decisions; no repository language
sources were read to supply these application requirements.
