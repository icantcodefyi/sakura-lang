# Bound an agent loop and its budget

Set both a turn limit and a spending limit, and test rejection before an unaffordable call.

`Agent.run` permits at most `max_steps` model turns and offers no tools on the last
turn. The tests verify that a model still requesting a tool then fails with
`InvalidOutput`. `BudgetExceeded`, other `LlmError` variants and `ToolError` propagate
in the declared failure union.

For finite accounting, put `Budget.limits` outside `Llm.anthropic` and replay HTTP
under `Http.recording` with an empty scripted fallback. Do not substitute
`Llm.replay` or `Llm.recording` here: those helpers install `Budget.unlimited`
internally. The placeholder `Secret` is not an API key, and no host HTTP handler or
network permission is installed.

The cassette's two turns report synthetic usage of 140 tokens each. The provider
reserves an estimate and settles that usage; the agent also holds a temporary
one-call reservation while a turn is in progress. Consequently the two-call run
needs a call limit of three for its overlapping reservations. A limit of two
rejects the second provider call. The tests exercise both cases, a token limit that
rejects before any scripted response is consumed, and the final-turn rule. These
are local accounting tests, not measured provider prices or a hard billing guarantee.
The harness uses test mode; no key, paid call or live service is involved.
