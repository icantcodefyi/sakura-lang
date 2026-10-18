# Decode JSON into a refined record

Decode an order once and let its field refinements reject invalid external data.

`Json.decode` infers `Order` from the return annotation and checks both the record
shape and the predicates on `item` and `quantity`. Its `Result` keeps malformed
JSON, missing fields and invalid quantities recoverable; `main` handles both
branches and reports the decoding path. Use `from` only for trusted values where a
failed predicate should be a defect. No permissions are needed.

The harness runs `main` (expecting `2 x tea`) and both tests. The tests cover a valid
record, invalid JSON, an empty name, missing quantity and both out-of-range bounds.
