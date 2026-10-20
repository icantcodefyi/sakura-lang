# Synthetic supplier cassettes

Hand-authored by Claude Opus 5.5 on 2026-10-21 (generated with a short Python
script from literal offer tables) in the version 1 cassette format. No live
request, supplier, account, credential or personal data was used. The hosts
`supplier-{a,b,c}.invalid` are fixture identities, not services.

Matching rules (Http.recording, sakura 0.1.0): an exchange matches on method,
URL, ordinary headers and body. Every request here is
`GET https://supplier-X.invalid/stock` with no headers and an empty body.
Identical requests replay in recorded order, the last one repeating. Tests
install an empty `Http.scripted([])` outside each recording, so a miss fails
offline as a transport error.

| File | Exchanges |
| --- | --- |
| `supplier-a.jsonl` | 200, five USD offers (W-1, W-2 short, W-3, W-5 zero stock, FREE-1 at price 0) |
| `supplier-b.jsonl` | 200, three USD offers (W-1, W-2, W-3 tying supplier-a's price) |
| `supplier-c.jsonl` | 200, three USD offers (W-1 tying supplier-b, W-2, W-5 short) |
| `supplier-a-429-then-200.jsonl` | 429 `slow down`, then the same 200 as `supplier-a.jsonl` |
| `supplier-b-invalid-utf8.jsonl` | 200 whose `response_body_base64` holds bytes 0xFF 0xFE inside a JSON string |

Each test installs its own cassette inside each supplier's child task, so the
three suppliers replay independently.
