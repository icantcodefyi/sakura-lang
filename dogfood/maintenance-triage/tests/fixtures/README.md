# Synthetic provider cassettes

Every `*.jsonl` file here is a hand-specified, synthetic Anthropic Messages
exchange in the version 1 cassette format. No live request, provider account,
API key or recorded personal data was used. Response IDs (`msg_SYNTHETIC_n`,
`toolu_SYNTHETIC_n`), text and token usage (120 input / 20 output per turn) are
invented. Secret headers are never written; the placeholder key
`offline-placeholder-key` appears in no file (test 26 checks this).

| Cassette | Turns | Scenario |
| --- | --- | --- |
| `two_turn.jsonl` | fetch, answer | success; the documented two-turn budget case (tests 17, 18, 26, 30) |
| `hazard.jsonl` | search, fetch, answer | smoke incident; the model proposes a repair, policy escalates (tests 29, 30) |
| `refusal.jsonl` | 1 | `stop_reason: "refusal"` (test 14) |
| `truncated.jsonl` | 1 | `stop_reason: "max_tokens"` with a cut-off answer (test 15) |
| `malformed.jsonl` | 1 | answer object missing required fields (test 15) |
| `tool_error.jsonl` | 1 | fetch of an unknown entry (test 28) |
| `injection.jsonl` | fetch, answer | injection-like incident; only the two manual tools are offered (test 22) |

## Regeneration

The request side of each exchange was produced by the real `Llm.anthropic`
adapter, so it matches its serialization exactly; the response side comes from
`synthetic_provider` in `src/main.sak`, a test `Http` handler that returns the
listed bodies in order without touching the network.

```sh
rm tests/fixtures/*.jsonl
env -u ANTHROPIC_BASE_URL sakura test --record --filter regenerate
env -u ANTHROPIC_BASE_URL sakura test
```

Delete a cassette before regenerating it: under `--record` only misses are
forwarded, and the in-order synthetic provider assumes it sees every turn.
`ANTHROPIC_BASE_URL` must be unset (see FRICTION.md): the adapter reads it
from the process environment and it changes the recorded URL.

A changed system instruction, tool description, manual fixture or answer
schema changes the request bodies; replay then fails as a cassette miss and
the cassettes must be regenerated and reviewed.

`data/drill-replay.jsonl`, `data/smoke-replay.jsonl` and
`data/refusal-replay.jsonl` are copies of `two_turn.jsonl`, `hazard.jsonl` and
`refusal.jsonl` for the CLI's offline `--replay` mode.
