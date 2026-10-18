# 09-scripted-reading

The visible scripted LLM test fails: encode_reading emits the wrong JSON shape. Repair it to encode the Reading record expected by read_script. Preserve read_script and its LlmError signature. The scripted model must work for any tested integer value, with no provider, secret or HTTP handler.

Edit only `src/main.sakura`. Preserve existing declarations, signatures, contracts,
and the visible test; imports and implementation bodies may be repaired. Keep
`sakura.toml` and its permissions unchanged. Finish all holes, avoid `@trust`,
externals and host handlers, and check and test the project. Hidden acceptance
checks additional inputs. No live services, credentials or network are needed.
