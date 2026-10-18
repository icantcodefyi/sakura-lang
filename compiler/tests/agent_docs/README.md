# Skill fixtures

Expectations for the code fences of `skills/sakura/` (design §17.142), checked by
`crates/sakura_cli/tests/agent_docs.rs`; its header documents the fence modes. Files:

- `<id>.stdout`: what a `sakura run id=<id>` sample prints;
- `<id>.span`: the source text under the primary span of a `sakura wrong:<code> id=<id>` sample;
- `<id>.fixed.sak`: the repaired program of a wrong sample whose fix is not itself a fence.

`SAKURA_BLESS=1 cargo test -p sakura_cli --test agent_docs` writes `.stdout` and `.span` files
from the actual output; review every one. A file no fixture uses fails the test.
