# Run an agent loop with a tool

Give a model a typed tool and test the whole loop offline against a recorded two-step run.

`Tool.of` turns a named `@tool` function into a tool whose arguments are decoded against its
parameter types; `Agent.run` lets the model call it and returns the final answer as a typed
record. The test replays `tests/fixtures/research.jsonl`, a synthetic two-step cassette (a recorded exchange: a
tool turn, then the answer), through `Llm.replay`, the Anthropic adapter over the cassette, with the knowledge base the tool searches
answered by `Http.scripted`, so it needs no key and no network.

`main` calls a real model, so only the test is run here. The fixture's bodies, IDs and
token counts were invented (see its README); this is not live-provider validation.
