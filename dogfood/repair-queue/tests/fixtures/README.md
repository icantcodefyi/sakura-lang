# Fixtures (synthetic)

`client-session.jsonl` is a **synthetic** HTTP cassette. No live server or
network produced it. It was recorded on 2026-10-21 with `sakura test --record`
(sakura 0.1.0, 2014e821b87a0d30cb8061b6158f99723f0f676d) from a throwaway test
whose outer `Http` handler called this service's own `Router.handle(app(), req)`
over a fresh `Sql.memory()`, so the request keys match the real client's
serialization. That throwaway test is not committed.

Exchanges, in order:

1. `POST http://127.0.0.1:8787/tickets` creating `VAC-12`: 201 with the new ticket.
2. `GET http://127.0.0.1:8787/tickets/VAC-12`: 200 with the same ticket.
3. `POST http://127.0.0.1:8787/tickets` with `VAC-12` again: 409 `duplicate`.

`src/cassette.sak` replays it with an empty `Http.scripted([])` fallback, sends
the same requests through the actual router and compares the decoded records,
and checks that a request missing from the cassette fails offline. If the
service's responses change, that comparison fails: regenerate the cassette
deliberately and review the diff.
