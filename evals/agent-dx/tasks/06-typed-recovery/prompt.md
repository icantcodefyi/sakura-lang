# 06-typed-recovery

Fill `recover`: call the existing integer parser and return its value on success; on BadCount return the caller-provided fallback. Preserve the typed error and parser. Invalid, blank and fractional strings must be recoverable; a valid negative integer is not an error.

Edit only `src/main.sakura`. Preserve existing declarations, signatures, contracts,
and the visible test; imports and implementation bodies may be repaired. Keep
`sakura.toml` and its permissions unchanged. Finish all holes, avoid `@trust`,
externals and host handlers, and check and test the project. Hidden acceptance
checks additional inputs. No live services, credentials or network are needed.
