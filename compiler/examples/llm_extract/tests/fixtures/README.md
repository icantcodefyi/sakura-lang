# Synthetic cassettes

These are **hand-authored, synthetic** HTTP cassettes, not recordings of a
real service: no request was ever sent to a provider, and the response bodies, ids and token
counts are invented to match the documented response shapes. They contain no secrets: secret
headers are never written to a cassette or used to match one. The first line is the
cassette header `{"schema_version":"1.0","kind":"cassette"}`.

`extract_ticket.jsonl`: two Anthropic Messages API exchanges for `extract_ticket` (structured
output, `stop_reason: "end_turn"`): the default model for the `Llm.replay` test and
`claude-sonnet-5-5` for the `Llm.recording` test.

Each request line (method, url, headers, body) was produced by running the test once under
`sakura test --record` with the outer `Http` scripted to answer the synthetic response, so the key
matches what the program sends byte for byte. If a change to the program, std or the runtime
changes a request, the test fails with `no recording for …`; regenerate the line the same way
(or, with a real key, `sakura test --record`) and review the diff.
