# Retry HTTP with an offline cassette

Retry a safe GET after a rate limit, with a finite attempt count and virtual time in tests.

`fetch` declares HTTP, clock and random effects. Its policy retries only
`RateLimited`, at most three attempts, with exponential backoff and a one-second
retry deadline. Jitter is disabled for this example. It uses the policy's delays;
it does not interpret `retry_after`. The deadline bounds retry scheduling, not the
duration of an individual request. `HttpError` propagates immediately. Other HTTP
statuses remain responses for the caller to inspect.

The cassette returns 429 then 200 for the identical request. `Http.recording` is
wrapped by `Http.scripted([])`, so an exchange cannot fall through to the network,
even in recording mode. `Clock.virtual` advances without sleeping. Tests check
success after 100 ms, exhaustion after 300 ms, and immediate transport failure.
The harness uses test mode; the manifest grants no network or environment access.
