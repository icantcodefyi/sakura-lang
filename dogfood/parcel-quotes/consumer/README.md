# Node consumer for Parcel Quotes

Calls the library's `quote` entry from Node through `callEntry`, as the page
"Embedding a library in Node" (`/docs/tour/embedding`) documents. Written by Claude
Opus 5.5 from that page alone, against the source build of main, 2026-10-21.

## Run it

From `dogfood/parcel-quotes/`:

```sh
sakura build --target wasm --out-dir /tmp/sakura-pq-consumer/wasm
node consumer/consumer.mjs /tmp/sakura-pq-consumer/wasm

# the same consumer on the JavaScript build
sakura build --target js --out-dir /tmp/sakura-pq-consumer/js
node consumer/consumer.mjs /tmp/sakura-pq-consumer/js
```

It prints a transcript and exits 1 if a check fails. It sets `SAKURA_PERMISSIONS` to
the manifest's table with every key written, `{"fs":{"read":[],"write":[]},
"net":["carrier.invalid:443"],"env":[]}`, before the first call. It
contacts no carrier: `carrier.invalid` never resolves (RFC 6761).

## What it shows

- **Entries:** `quote` is listed with its parameter, result and `Fail` errors.
- **Invalid input, no HTTP work:** zero, over-limit weight, an unknown zone and a bad
  service ID each answer `{ kind: "fail" }` with the typed error. Each call settles
  without waiting on the host. With network granted, a request would end in
  `CarrierUnreachable`.
- **Suspension and resumption:** a valid parcel's call is still pending after a
  macrotask while the host looks up the carrier's name. It then resumes, and the host's
  failure reaches Node as the library's typed `CarrierUnreachable { reason }`.
- **Typed values across the boundary:** Node builds test 01's oracle `Outcome` (870 EUR,
  standard, eu). The `render_outcome` entry renders it to the same line as
  `sakura run -- 1000 eu standard < samples/rates.json`. Test 09's `NoQuote` crosses the
  boundary intact too.
- **Two independent invocations:** two Hosts called concurrently each answer their own
  input. A repeated call gives the same answer.
- **Refused calls:** a missing argument, a number, a string, a `ParcelInput` whose
  weight is a string, or a value already wrapped in `Untrusted` throws before the
  function runs.
- **Denied network authority:** a child Node process (the same consumer, loading the
  module in-process, with `--denied`) sets `net` to `[]`, and a second one (`--empty`)
  sets `{}`, where a key left out is denied. `quote` fails with
  `CarrierUnreachable { reason: "carrier.invalid:443 is not in the manifest's net list" }`.
  A child is needed because the host reads the variable once per process.

## What crosses the boundary

In: one plain `ParcelInput` (`new ParcelInput(weight_g, zone, Option)`), which `quote`
receives as `Untrusted<ParcelInput>`. Out: an outcome object holding either an `Outcome` value (`Quoted(List<Quote>,
List<Exclusion>)` or `NoQuote(...)`) or one of the ten `Fail` errors, with `shown` text.
These are the runtime's own objects; nothing is JSON. HTTP never crosses: the host does
the request itself, under the permission ceiling. Node never sees or supplies it.

Release: the page documents no release step, so the consumer calls none. Each call ends
when its promise settles. The consumer ends with `process.exitCode`, not
`process.exit()`, and Node still exits, so no Host keeps the event loop alive.

## What it does not show

- **A quote from `quote` itself.** The page has no way to give an entry's root `Http`
  handler a synthetic response. The only real response would come from a live carrier.
  So the oracle comparison goes through `render_outcome` and not through `quote`.
- **A suspended request answered by Node** (the brief's pull boundary): Node never sees
  the pending request. The suspension shown is the host's own name lookup.
- **Leakage of rate data between calls:** no call gets rate data, so none can leak. The
  Sakura test 27 covers this with scripted handlers.

See `../FRICTION.md`, section "2026-10-21 — Node consumer from the embedding page".
