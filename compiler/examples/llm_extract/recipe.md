# Extract a typed record from model output

Ask a model for a refined record type and test the extraction offline, scripted and replayed.

An `@llm` function declares the record it returns; the adapter asks the model for that type's
JSON Schema and decodes the answer against the type, refinements included, so a wrong answer
is the typed failure `InvalidOutput`, never a crash. The first two tests script the model with
`Llm.scripted`, which takes the answer itself. The other two replay
`tests/fixtures/extract_ticket.jsonl`, a synthetic Messages API exchange, through
`Llm.replay` and through `Llm.recording`, whose test `sakura test --record` would fetch once.

`Llm.anthropic` always sends to `https://api.anthropic.com`, so the cassette's keys are the
same on every machine; for another endpoint pass it with `Llm.anthropic_at(base, key, model)`
(no process variable is read).

`main` calls a real model, so only the tests are run here. The fixture's bodies, IDs and
token counts were invented (see its README); this is not live-provider validation.
