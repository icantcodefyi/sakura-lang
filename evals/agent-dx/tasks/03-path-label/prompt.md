# 03-path-label

Repair the missing Path import and the return-type mismatch in `label`. Normalize the valid relative path with Path.of, then return `<normalized> (<n> bytes)` using its UTF-8 byte length. All inputs satisfy Path.is_relative. Do not remove the path normalization to silence the diagnostic.

Edit only `src/main.sakura`. Preserve existing declarations, signatures, contracts,
and the visible test; imports and implementation bodies may be repaired. Keep
`sakura.toml` and its permissions unchanged. Finish all holes, avoid `@trust`,
externals and host handlers, and check and test the project. Hidden acceptance
checks additional inputs. No live services, credentials or network are needed.
