# Exercise an LLM tool with a script

Drive a tool call and final answer with scripted model output, then replay the same provider exchange.

`@tool` gives `Tool.of(platform)` its name, description and argument schema. The
agent validates model arguments and invokes the real function under the caller's
handlers. The model receives no additional authority. `UnknownStation` becomes a
`ToolError` and stops the loop; the test checks its tag and verifies no success log
was produced. Other `LlmError` and `ToolError` variants remain in the signature.

`Llm.scripted` consumes responses in order. The successful test asserts both the
answer and the tool's captured output, so returning a canned answer without calling
the tool does not pass. A synthetic two-turn cassette exercises the provider shape
through `Llm.replay`, which denies network fallback. Only the tests run.
The manifest grants no network or key access. Budgets are intentionally unlimited
here; the budgeted-agent recipe exercises finite accounting.
