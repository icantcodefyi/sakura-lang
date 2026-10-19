# Parcel Quotes: a Sakura WASM library for a Node storefront

- Brief author: GPT-6-Astra (Codex family), 2026-10-19.
- Status: website-only brief, before implementation; embedding integration is blocked on public documentation (G02).
- Required domain: WASM library consumed from Node across the pull boundary.
- Intended program author: Claude Opus 5.5 (Claude family), one fresh session with no repository checkout or path to one; Opus also authors any Node consumer code.
- Initial release: installed `v0.1.0-alpha.1`; actual version/build revision pending.
- Program header must record actual author, fresh-session provenance and release built against.

## Purpose and user-visible behaviour

A Node storefront wants reusable shipping-policy logic without rewriting that
logic in JavaScript. Its checkout calls the Sakura library with a parcel's weight,
destination zone and requested service. The library obtains a carrier-rate snapshot
through an HTTP effect, validates it, computes eligible quotes and returns a typed
result for the Node caller to render. Node is the consuming application, not merely
a shell launching a Sakura CLI.

Return eligible services ordered by total price and then service ID. Also return
excluded-service reasons such as unknown zone, excessive weight or unavailable
service. Prices use integer minor currency units. Each service defines a base
charge, charge per started kilogram and optional zone surcharge. A customer chooses
later; this library never purchases a label or creates a shipment.

The integration must exercise the required pull boundary: a library invocation
that needs host work suspends, the Node host fulfils the pending HTTP operation,
and the invocation resumes with its result. This is a behavioural requirement from
the corpus decision, not a claim that the website documents a callable embedding
API. A pure WASM arithmetic export or `sakura run --target wasm` alone does not
satisfy the domain.

## Inputs, outputs and limits

Input is a schema-validated parcel record: weight 1–30,000 grams, one of three
configured zone IDs and an optional requested service ID. Do not transport an
integer outside the common JS safe range. The carrier snapshot has at most ten
distinct service IDs, bounded names, positive weight limits, nonnegative prices
at most 1,000,000 per charge component, and the single application currency.
Reject duplicate service/zone entries and wrong currency instead of picking one.

Read at most 65,536 response bytes, decode UTF-8 and JSON with recoverable errors,
and validate before calculating. Round weight up to whole kilograms: 1–1,000 g
uses one unit, 1,001–2,000 g uses two. Cap all application arithmetic within the
shared integer range. A request that matches no service returns an explicit
no-quote result; invalid input is distinct from a valid but unserviceable parcel.

For offline evidence use `https://carrier.invalid/rates`, an invented fixture
endpoint. A deployment chooses one actual carrier endpoint and grants that exact
host/port. No URL is taken from a customer or a returned rate document. No retries
are required; transport failures and non-200 responses are explicit failures.
Use a five-second cooperative timeout for the effectful fetch.

## Std surface, host boundary and permissions

Use `Http.stream`/`read_chunk`, `Http.scoped`, `Json`, refinements, `Untrusted`
validation, typed failures, `Task`/`timeout`, `Clock`, `List` and integer arithmetic.
Separate pure validation/calculation from the effectful rate lookup. Test the
same policy under `js` as well as `wasm`; the Node consumption requirement applies
specifically to the WASM artifact.

The Sakura operation requires network authority for one carrier host only. It
needs no `Fs`, `Env`, secret, write or process capability. The Node consumer needs
to load the emitted module and its matching bundled host files from the output
directory. That host file access is not a reason to give Sakura application code
filesystem authority. Tests replay the carrier cassette using the fixture `Dir`.

The website warns that directly running emitted code without `SAKURA_PERMISSIONS`
does not establish the manifest ceiling. The eventual public embedding recipe must
therefore explain how the Node consumer sets and enforces the same grants, how it
invokes a library entry, transports typed values/errors, services pending work and
releases the invocation. Do not guess export names, read compiler/runtime sources,
use `@external(js, ...)` to bypass WASM's restriction, or mix a module with another
release's host. G02 is an implementation blocker for this integration until the
published docs supply the recipe; it does not justify dropping the requirement.

## Acceptance: at least 20 separately named sakura test cases

These Sakura cases establish the library's policy and effect behaviour. They are
required even though separate Node integration evidence is also necessary.

1. A 1,000 g parcel pays one weight unit plus its base charge and zone surcharge.
2. A 1,001 g parcel pays two weight units.
3. A one-gram parcel receives the minimum one-unit quote.
4. A 30,000 g parcel is admitted when a service's weight limit allows it.
5. Zero weight is rejected at input validation without a contract defect.
6. A weight above 30,000 g is rejected before an HTTP call.
7. Unknown customer zones are invalid input, distinct from a valid zone unsupported by a service.
8. A requested known service filters the result to that service.
9. A requested absent service returns an explicit no-quote result.
10. A service below the parcel's weight is excluded with the correct reason.
11. A zero base charge and zero surcharge remain valid.
12. Equal prices sort by service ID, independently of snapshot order.
13. Negative or over-limit prices reject the snapshot before calculation.
14. Duplicate service IDs reject the snapshot.
15. Duplicate zone entries in one service reject the snapshot.
16. A currency mismatch rejects the snapshot.
17. An empty valid snapshot produces a no-quote result, not a decode failure.
18. Eleven service records exceed the bound; ten are accepted.
19. Invalid UTF-8 and malformed JSON produce distinct recoverable input failures.
20. The response byte cap is checked while reading; cap-plus-one fails before parsing.
21. The carrier success cassette yields the same typed quote as a directly supplied snapshot.
22. A cassette miss fails offline with no host fallback.
23. A non-200 response fails explicitly and is not parsed as a rate document.
24. A virtual-time delayed fetch times out, joins its child and returns a typed timeout.
25. A narrowed carrier handler refuses a different host before forwarding.
26. Maximal allowed charges and weight produce exact, equal prices on JS and WASM.
27. Repeating a call with fresh handlers cannot inherit the previous request's rate data.

Use `Http.recording`, `Fs.rooted(Test.fixtures())`, an empty scripted outer HTTP
handler, fixed seeds and virtual time. Commit the carrier's synthetic cassette
and its provenance. Keep per-request state inside the request's handler scope.

## Additional Node consumption acceptance

After G02 is resolved on the public site, Opus implements a small Node consumer
that imports/loads the built library through the documented entry mechanism. It
must demonstrate a quote round trip, an actually suspended host request followed
by resumption, an invalid input with no HTTP work, a host failure translated to
the caller, two independent invocations without state leakage, and denied network
authority. Record what crosses the pull boundary and how resources are released.
Use only a synthetic host response/cassette, not a live carrier. Compare the returned
quote with the Sakura test oracle. Do not claim the 20-plus Sakura tests establish
this Node integration or that a command-line subprocess is a library consumer.

## Delivery and release evidence

Place the project at `dogfood/parcel-quotes/`. Require at least 300 substantive
non-test `.sak` lines, excluding any Node consumer from that count, and at least
20 Sakura tests. Validation, pricing, exclusion reasons, adapters and error mapping
provide the program scope. Opus works from installed alpha.1, its setup skill and
the website only. Record the embedding blocker and any alpha.1 limitation; do not
use a source build as acceptance. CI must eventually pass on `js` and `wasm` with
installed `v0.1.0`, plus the Node consumption checks. Keep module and host together.
After 0.2.0 tags, Astra writes the release-notes-only `MIGRATION.md` before Opus
edits; every diff hunk must be listed. Later Astra review edits no code.

## Non-goals and open friction

No browser/WASI host, custom ABI implementation, raw memory interface, shipment
purchase, credentials, currency conversion, package dependency or stable generated
byte format. G01, G02, G04 and G10 in [the gap register](../website-gaps.md) are
relevant. The Node integration is intentionally specified but unimplemented and
unverified; an undocumented API must remain a gap rather than become invented code.

## Website evidence

Read on 2026-10-19:

- [Backends](https://www.sakuralang.com/docs/tour/memory#backends): emitted WASM/Node files, target flags and external-binding limits.
- [CLI](https://www.sakuralang.com/docs/cli): library project kind, outputs and runtime prerequisites.
- [Capabilities](https://www.sakuralang.com/docs/tour/capabilities): direct-host permission caveat and narrowing.
- [Std reference](https://www.sakuralang.com/docs/stdlib), [contracts](https://www.sakuralang.com/docs/tour/contracts) and [concurrency](https://www.sakuralang.com/docs/tour/concurrency): APIs used by the application core.
- [Cassettes](https://www.sakuralang.com/docs/agents/cassettes) and [testing](https://www.sakuralang.com/docs/tour/testing): offline acceptance.
- [Site-wide docs export](https://www.sakuralang.com/llms-full.txt): searched for a public embedding/pull-boundary recipe; none was found.
- [Install](https://www.sakuralang.com/docs/install), [changes](https://www.sakuralang.com/docs/changes) and [changelog](https://www.sakuralang.com/changelog.md): release baseline and host/module compatibility.

The required pull-boundary domain comes from the supplied decision, not from an
inferred website API. No linked repository ABI or compiler file was opened.
