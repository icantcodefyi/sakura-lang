# 11-successor-contract

Repair successor so its existing postcondition holds for every valid input. Preserve both named contracts verbatim and keep invalid inputs rejected. The intended operation is x + 1, including at zero and 99. Do not use trust or alter the visible test.

Edit only `src/main.sakura`. Preserve existing declarations, signatures, contracts,
and the visible test; imports and implementation bodies may be repaired. Keep
`sakura.toml` and its permissions unchanged. Finish all holes, avoid `@trust`,
externals and host handlers, and check and test the project. Hidden acceptance
checks additional inputs. No live services, credentials or network are needed.
