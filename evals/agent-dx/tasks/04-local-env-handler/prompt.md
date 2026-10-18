# 04-local-env-handler

Repair the undeclared Env effect without changing the pure outer signature.
Run the supplied read callback inside a local deterministic environment containing
MODE=quiet and REGION=west; every other key must be absent. Return the callback's
own String result, including any transformation it performs. Do not read the
process environment or broaden permissions.

Edit only `src/main.sak`. Preserve existing declarations, signatures, contracts,
and the visible test; imports and implementation bodies may be repaired. Keep
`sakura.toml` and its permissions unchanged. Finish all holes, avoid `@trust`,
externals and host handlers, and check and test the project. Hidden acceptance
checks additional inputs. No live services, credentials or network are needed.
