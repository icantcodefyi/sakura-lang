# Repair Queue: a local repair cafe's intake service

- Brief author: GPT-6-Astra (Codex family), 2026-10-19.
- Status: website-only brief, before implementation; no execution evidence yet.
- Required domain: HTTP service over `Serve` and `Sql.sqlite`.
- Intended program author: Claude Opus 5.5 (Claude family), one fresh session with no repository checkout or path to one.
- Initial release: installed `v0.1.0-alpha.1`; actual version/build revision pending.
- Program header must record actual author, fresh-session provenance and release built against.

## Purpose and user-visible behaviour

A repair cafe needs a small local service for recording an item's intake, assigning
a repair station and tracking its progress through one session. A ticket contains
an operator-supplied ID, item description, station and state. Store no customer
names or contact details. Persist the queue in SQLite so restarting the service
does not discard the session.

Offer JSON routes to create a ticket, fetch one, list the queue, assign a station
and change state. The states are waiting, in-progress, completed and withdrawn.
Allow waiting → in-progress or withdrawn, and in-progress → completed or withdrawn.
Repeating the current state is an idempotent success; terminal states cannot
reopen. Every genuine transition appends one history entry in the same transaction
as the state update. An expected-state field rejects stale updates with 409.

Return 201 for creation, 200 for reads and accepted updates, 400 for invalid input,
404 for an unknown ticket or route, 405 for a wrong method and 409 for duplicate IDs
or state conflicts. Database failures produce a generic 500, without SQL text or
filesystem details. List tickets in ID order and history in explicit sequence order.

## Inputs, outputs and bounds

IDs are 1–32 ASCII letters, digits or hyphens; descriptions are 1–200 UTF-8 bytes;
station is one of intake, electrical, textiles or mechanical. Ticket creation
starts at intake/waiting. Bodies are limited to 4,096 bytes at `Request.json`;
the application's 400 response is separate from the host's documented 1 MiB/413
limit. Cap a cafe session at 500 tickets, with an explicit capacity-conflict reply.
Fixed routes and typed request records avoid accepting arbitrary SQL or table names.

Responses contain typed JSON records, including state and history. Expected state
and duplicate detection are enforced in the transaction, not by a separate
read-before-write promise. Keep the schema/migration small and versioned by this
application; startup is repeatable on an existing database.

## Std surface and permissions

Use `Router`, `Request.json`/`param`, `Response.ok_json`/`status`, `Serve.listen`,
`Sql.sqlite`, `Sql.query`/`execute`/`transaction`/`migrate`, bound `sql` templates,
`Json`, `Fs` and a confined `Dir`. Domain failures distinguish unknown tickets,
duplicates, stale updates and invalid transitions; map them at the HTTP boundary.
Keep transition policy pure so it can be tested separately from SQL and serving.

Listen only on `127.0.0.1:8787`. Grant that exact `net` pair and filesystem reads
and writes only under `./data`, where `repair-queue.sqlite` lives. There is no
outbound HTTP, environment access or process launch. The listener must use the
same host spelling as the grant. This unauthenticated service is for loopback use;
remote access and production deployment are outside the brief.

## Acceptance: at least 20 separately named sakura test cases

Exercise the actual router through `Router.handle` and a fresh `Sql.memory()` per
test. Check response bodies as decoded records and inspect resulting database
state; asserting status alone is insufficient.

1. Creating a valid ticket returns 201 with its ID, intake station and waiting state.
2. Fetching that ticket returns exactly its stored description and state.
3. Listing tickets sorts by ID regardless of insertion order.
4. Creating an existing ID returns 409 and preserves the original row.
5. A malformed JSON body returns 400 without inserting a row.
6. A missing required field returns 400 without a database write.
7. An empty description is rejected and a 200-byte description is accepted.
8. A 201-byte description, including a multibyte boundary case, is rejected.
9. A 4,097-byte body is rejected before application storage.
10. An invalid ID alphabet is rejected without turning it into a path or query.
11. An unknown station is rejected; each documented station is accepted.
12. Assignment changes the station while preserving the ticket ID and description.
13. Waiting → in-progress appends exactly one history entry.
14. In-progress → completed appends the next entry in stable order.
15. Waiting → withdrawn succeeds and is terminal.
16. In-progress → withdrawn succeeds and is terminal.
17. A terminal → waiting request returns 409 with unchanged history.
18. Repeating the current state succeeds without adding duplicate history.
19. A stale expected-state request returns 409 without overwriting a newer state.
20. Failure during a transaction rolls back both a state change and its history insertion; use a controlled SQL failure rather than weaken the transaction.
21. SQL-looking description text is stored literally and cannot broaden a query.
22. Unknown-ticket, unknown-route and wrong-method requests produce their specified distinct responses.
23. Initialisation repeated on the same database preserves tickets and avoids duplicate schema work.
24. The 500th ticket is accepted; a 501st returns capacity conflict without insertion.
25. A simulated `SqlError` at the route boundary yields a generic 500 without database paths or query text.

Commit a synthetic HTTP request/response cassette for this service and replay a
client create/read/conflict sequence offline with `Http.recording` and an empty
scripted fallback. Separately run those same requests through the actual router
and compare the expected response records: a canned cassette alone cannot test
the server. Add a cassette-miss test that fails offline. Label fixtures synthetic.

Additional installed-binary integration acceptance must start the real listener,
create a ticket, restart with the same `Sql.sqlite` file and retrieve it. Verify
denial with missing listener/filesystem grants. Ordinary `sakura test` has no host
permissions, so `Sql.memory` tests do not establish persistent-file or socket
behaviour. G03 and G05 document the missing public recipe for this last step.

## Delivery and release evidence

Place the eventual project at `dogfood/repair-queue/`: at least 300 non-test `.sak`
lines and at least 20 tests, including substantive domain policy, persistence,
validation and HTTP translation. Opus uses the installed binary, its setup skill
and the website only. Start on alpha.1; record unavailable features rather than
using a source build. All acceptance tests must pass in CI on `js` and `wasm`
against installed `v0.1.0`. Record the first migration from the published change
notes. For 0.2.0, Astra supplies `MIGRATION.md` from release notes before Opus edits;
every resulting hunk must be listed. Astra later reviews without editing code.

## Non-goals and open friction

No login system, internet exposure, payments, uploads, PostgreSQL, background jobs,
multi-server consistency, throughput claim or general workflow engine. Do not
turn application acceptance into a promise about SQLite filesystem-race protection.
G01, G03, G04 and G05 in [the gap register](../website-gaps.md) remain explicit;
especially, public in-memory examples do not establish alpha.1 persistence setup.

## Website evidence

Read on 2026-10-19:

- [Router recipe](https://www.sakuralang.com/docs/cookbook/router_endpoint): direct route tests and bounded JSON requests.
- [SQLite recipe](https://www.sakuralang.com/docs/cookbook/sqlite_query): bound parameters, typed rows and rollback.
- [Std reference](https://www.sakuralang.com/docs/stdlib): `Serve`, `Router`, `Sql.sqlite` and filesystem surfaces.
- [Handlers](https://www.sakuralang.com/docs/tour/handlers) and [effects](https://www.sakuralang.com/docs/tour/effects): replaceable handlers and typed error boundaries.
- [Manifest](https://www.sakuralang.com/docs/manifest) and [capabilities](https://www.sakuralang.com/docs/tour/capabilities): root grants and their limits.
- [Cassettes](https://www.sakuralang.com/docs/agents/cassettes) and [testing](https://www.sakuralang.com/docs/tour/testing): offline evidence.
- [Install](https://www.sakuralang.com/docs/install), [changes](https://www.sakuralang.com/docs/changes) and [changelog](https://www.sakuralang.com/changelog.md): release baseline and server changes.

Only the supplied decision texts establish corpus workflow requirements; all
language/API research for this brief used the website, not repository sources.
