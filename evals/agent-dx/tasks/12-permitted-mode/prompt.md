# 12-permitted-mode

Repair mode to use the already permitted APP_MODE key instead of ADMIN_MODE. Return "fast" only when APP_MODE is exactly "fast"; missing, empty or other values return "safe". Never consult ADMIN_MODE, grant new environment keys, install a host handler or bypass attenuation. Tests simulate the manifest grant with Env.only.

Edit only `src/main.sakura`. Preserve existing declarations, signatures, contracts,
and the visible test; imports and implementation bodies may be repaired. Keep
`sakura.toml` and its permissions unchanged. Finish all holes, avoid `@trust`,
externals and host handlers, and check and test the project. Hidden acceptance
checks additional inputs. No live services, credentials or network are needed.
