# Synthetic cassettes

These are **hand-authored, synthetic** HTTP cassettes (design §17.73), not recordings of a
real service: no request was ever sent to a provider, and the response bodies, ids and token
counts are invented to match the documented response shapes. They contain no secrets: secret
headers are never written to a cassette or used to match one. The first line is the
cassette header `{"schema_version":"1.0","kind":"cassette"}` (design §17.122).

`research.jsonl`: two Anthropic Messages API exchanges for `research`: a `tool_use` turn calling `kb_search`, then the structured answer. The knowledge-base request is not in the cassette; the test scripts it with `Http.scripted`.

Each request line (method, url, headers, body) was produced by running the test once under
`SAKURA_RECORD=1` with the outer `Http` scripted to answer the synthetic response, so the key
matches what the program sends byte for byte. If a change to the program, std or the runtime
changes a request, the test fails with `no recording for …`; regenerate the line the same way
(or, with a real key, `sakura test --record`) and review the diff.
