# 07-health-route

Add GET /health returning status 200 and body "ready", while preserving GET /version returning "v1". Use the Router API so other paths remain 404 and unsupported methods on a known path return 405. Tests dispatch directly; do not start a server.

Edit only `src/main.sakura`. Preserve existing declarations, signatures, contracts,
and the visible test; imports and implementation bodies may be repaired. Keep
`sakura.toml` and its permissions unchanged. Finish all holes, avoid `@trust`,
externals and host handlers, and check and test the project. Hidden acceptance
checks additional inputs. No live services, credentials or network are needed.
