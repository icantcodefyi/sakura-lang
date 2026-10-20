# Supplier Stock: reconcile availability before placing an order

- Brief author: GPT-6-Astra (Codex family), 2026-10-19.
- Status: website-only brief, before implementation; no tests or release runs claimed.
- Required domain: data pipeline over `Http`, with service cassettes.
- Intended program author: Claude Opus 5.5 (Claude family), one fresh session with no repository checkout or path to one.
- Initial release: installed `v0.1.0-alpha.1`; actual version/build revision pending.
- Program header must record actual author, fresh-session provenance and release built against.

## Purpose and user-visible behaviour

A small shop exports a wanted-items file and needs to know which of three suppliers
can fulfil it. Fetch their stock feeds concurrently, validate the feeds, and produce
a deterministic purchase-planning report. The program recommends suppliers; it
never places an order or contacts a customer.

For each wanted SKU, report requested quantity, offers that can supply the entire
quantity, the cheapest eligible offer, and the reason when none is available.
Prices are integer minor currency units in a single configured currency. Break
equal-price ties by supplier ID. Include supplier-level errors and a completeness
flag so an unavailable feed cannot masquerade as zero stock. Emit JSON to stdout.

Expected supplier failures become report data inside each child task, allowing
other suppliers to finish. Defects remain defects. Concurrency is substantive:
there are three independent feed requests, all inside one structured scope, with
no child escaping it. Do not infer parallel execution merely from a fast test.

## Inputs, outputs and resource policy

The wanted-items file is at most 65,536 bytes and contains at most 100 unique SKUs;
quantity is 1–1,000. A SKU is 1–40 ASCII letters, digits or hyphens. Reject duplicate
wanted SKUs rather than invent a merge policy. Each supplier has one bounded feed,
at most 100 offers, with SKU, available quantity (0–100,000), unit price
(0–1,000,000), currency and supplier ID. Reject a feed with a duplicate SKU,
incorrect supplier ID, wrong currency or invalid record; do not silently drop rows.

For reproducible examples the three endpoints are synthetic
`https://supplier-a.invalid/stock`, `https://supplier-b.invalid/stock` and
`https://supplier-c.invalid/stock`. They are fixture identities, not live services.
A real deployment supplies three operator-controlled endpoints and corresponding
exact host/port grants. Returned data cannot change these endpoints. No pagination,
redirect-based discovery or arbitrary URL input is part of the application.

Read at most 262,144 bytes of each response using `Http.stream`/`read_chunk`, with
a cap-plus-one check before JSON decoding. Treat a non-200 response explicitly.
Retry only HTTP 429: three total attempts, delays of 100 then 200 ms, no jitter.
Translate status to a typed rate-limit error at the adapter boundary; do not assume
`Http.stream` raises `RateLimited` as `Http.get` does. An individual supplier has a
five-second `Task.timeout` across all attempts. Retry scheduling alone is not a
request timeout. Reject malformed UTF-8 and JSON as feed errors, without retry.

## Std surface and permissions

Use `Cli`, bounded `Fs` reads, `Http.stream`, `Http.scoped`, `Task.scope`/`spawn`/`join`,
`timeout`, `retry`, `Clock`, `Random`, `Json`, refinements, `Untrusted` validation
and `List`/`Map`. Keep offer selection pure. Define typed supplier failures for
transport, status, timeout, malformed feed and bounds violations. Map those into
report entries at each child boundary rather than catching defects.

Grant `fs.read` only to `./data` and `net` only to the three configured `host:port`
pairs; no filesystem writes, environment access or subprocesses. Narrow each
supplier adapter with `Http.scoped` to its own host. Offline tests use fixture
capabilities and no live network or credentials. If supplier authentication later
becomes necessary it is a separate scope change, not a reason to widen this brief.

## Acceptance: at least 20 separately named sakura test cases

Use one synthetic cassette per supplier, `Fs.rooted(Test.fixtures())`, empty outer
`Http.scripted` fallback, `Clock.virtual`, a fixed random seed and
`Task.deterministic`. Give concurrent suppliers distinct URLs and independently
installed cassette handlers. Record fixture provenance and request matching rules.

1. Three healthy feeds produce the expected cheapest eligible supplier for every wanted SKU.
2. A cheaper supplier with insufficient stock loses to a supplier able to fill the whole quantity.
3. Equal eligible prices select the alphabetically first supplier ID.
4. A SKU missing from all successful feeds is reported unavailable, not omitted.
5. Zero stock is valid input but cannot fulfil a positive request.
6. A free item with sufficient stock remains an eligible zero-price offer.
7. A duplicate SKU in one feed rejects that feed and leaves the other suppliers usable.
8. An incorrect supplier ID rejects the feed instead of relabelling it.
9. A currency mismatch is visible as a supplier error, with no cross-currency comparison.
10. Malformed JSON from one supplier yields a partial report while two successful children complete.
11. Invalid UTF-8 is a feed error with no raw-body output.
12. Out-of-range stock or price is a validation error, never an arithmetic defect.
13. Exactly 100 offers pass the bound; 101 rejects the feed.
14. A response at the byte cap reaches decoding; one byte over is rejected without reading the remaining body.
15. A 429 then 200 cassette succeeds after one 100 ms virtual backoff.
16. Three 429 responses exhaust attempts after 300 ms of backoff and retain the rate-limit failure.
17. A 500 response is reported immediately without a retry.
18. A scripted transport failure is reported immediately, independently of the host's alpha.1 cut-body bug.
19. An effectful delayed supplier exceeds five seconds, is cancelled and joined, and contributes a timeout entry.
20. A scheduling-aware handler records all three requests starting before a gated response is released; all children are joined before reporting.
21. Reversing completion order leaves the report byte-for-byte unchanged.
22. A scoped adapter refuses an unrelated host before its permissive scripted outer handler is called.
23. A cassette miss fails offline and appears as a supplier error, never a fabricated zero-stock result.
24. Duplicate or invalid wanted SKUs fail before any supplier request.
25. The maximal valid wanted quantities and prices compute exact totals on both integer profiles.
26. A fully healthy run marks complete; any supplier failure marks incomplete and names that supplier.

Application exit status is 0 for a complete report and 1 for an incomplete report
or failed input. Test the report/status decision as data; verify the real process
exit separately. A virtual-time timeout test establishes cooperative effectful
cancellation, not preemption of arbitrary pure computation.

## Delivery and release evidence

The project belongs at `dogfood/supplier-stock/`, with at least 300 non-test `.sak`
lines and at least 20 tests. Parsing, bounded I/O, independent supplier adapters,
reconciliation, reporting and error translation should supply substantive scope.
Opus starts from the installed alpha.1, its setup skill and the website only.
Installed `v0.1.0` CI must pass on `js` and `wasm`, replaying every supplier cassette.
Record alpha.1 failures and note-backed migration changes; never substitute a
source build. After 1.0.0-beta.1 tags, Astra writes `MIGRATION.md` from notes alone before
Opus edits, and every diff hunk must be on that list. Later Astra review edits no code.

## Non-goals and open friction

No order submission, partial-order splitting, currency conversion, inventory
reservation, scraping, unbounded fan-out, durable scheduler, detached tasks or
performance claim. G01, G06 and G07 in [the gap register](../website-gaps.md) cover
release provenance, conflicting budget documentation and retry wording. This
pipeline uses its own explicit byte limits, not an undocumented byte-budget API.

## Website evidence

Read on 2026-10-19:

- [HTTP retry recipe](https://www.sakuralang.com/docs/cookbook/http_retry): finite attempts and retry-deadline limits.
- [Concurrency](https://www.sakuralang.com/docs/tour/concurrency): structured tasks, cancellation and deterministic scheduling.
- [Cassettes](https://www.sakuralang.com/docs/agents/cassettes): replay matching and scoped streaming bodies.
- [Std reference](https://www.sakuralang.com/docs/stdlib): stream chunks, `timeout`, JSON, UTF-8 and collection APIs.
- [Capabilities](https://www.sakuralang.com/docs/tour/capabilities) and [contracts](https://www.sakuralang.com/docs/tour/contracts): authority and input validation.
- [Testing](https://www.sakuralang.com/docs/tour/testing): handler-based cases and deterministic outputs.
- [Install](https://www.sakuralang.com/docs/install), [changes](https://www.sakuralang.com/docs/changes) and [changelog](https://www.sakuralang.com/changelog.md): baseline and alpha.1 transport caveats.

Corpus workflow constraints come from the supplied decisions; language research
comes only from the public website.
