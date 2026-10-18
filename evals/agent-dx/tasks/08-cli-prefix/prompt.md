# 08-cli-prefix

Extend `render` to honor optional --prefix. Parse Args using Cli.parse. Uppercase only the name when --upper is present, then prepend prefix (default empty). Preserve CliError for missing --name, unknown flags and missing flag values. The pure argument transformation must not read host arguments.

Edit only `src/main.sak`. Preserve existing declarations, signatures, contracts,
and the visible test; imports and implementation bodies may be repaired. Keep
`sakura.toml` and its permissions unchanged. Finish all holes, avoid `@trust`,
externals and host handlers, and check and test the project. Hidden acceptance
checks additional inputs. No live services, credentials or network are needed.
