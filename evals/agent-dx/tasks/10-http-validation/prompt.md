# 10-http-validation

Implement fetch_count: GET https://eval.invalid/count, decode a Count using Response.json with a 128-byte limit, return its value, and return -1 on any HTTP or decoding failure. Count.value must be 0..100. Keep the Http effect but handle all failures locally. Tests supply fake HTTP handlers, including invalid JSON and failed exchanges.

Edit only `src/main.sakura`. Preserve existing declarations, signatures, contracts,
and the visible test; imports and implementation bodies may be repaired. Keep
`sakura.toml` and its permissions unchanged. Finish all holes, avoid `@trust`,
externals and host handlers, and check and test the project. Hidden acceptance
checks additional inputs. No live services, credentials or network are needed.
