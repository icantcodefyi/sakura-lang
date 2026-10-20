# Fixtures

These fixtures are synthetic. No live carrier was contacted.

- `carrier_rates.jsonl` is a cassette for `GET https://carrier.invalid/rates` (`carrier.invalid`
  is a reserved, unresolvable name). The response body is `Json.encode(fixture_raw())` from
  `src/quote_tests.sak`: four invented services in EUR. It was written on 2026-10-21 by
  `sakura test --record` (sakura 0.1.0) from a temporary test. That test installed
  `Http.scripted([(Matcher.Any, Response.status(200, body))])` outside
  `Http.recording(Path.of("carrier_rates.jsonl"))` and called `quote`. The request key
  therefore matches the library's real request. The temporary test was then deleted.
- `carrier_miss.jsonl` has only the cassette header line. Every request misses, so test 22 can
  show that a miss fails offline and does not fall back to the outer handler.

To regenerate `carrier_rates.jsonl`, delete it, add the temporary test back and run
`sakura test --record --filter <its name>`. Review the diff before you commit it.
