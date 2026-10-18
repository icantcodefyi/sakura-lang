# Boundary and clock examples

Each block is a separate file. Run `sakura check --json <file>` and
`sakura test --json <file>`. These tests use no live services.

### Check a refinement at entry

Use `validate` for recoverable external input. Use `from` only at a boundary
where violation should be a defect; do not remove the predicate to accept it.

```sakura test id=skill-refinement-entry
type Percent = Int where it >= 0 && it <= 100
fn checked(n: Int) -> Percent { Percent.from(n) }
test "valid boundary" { expect_eq(checked(100), 100) }
test "invalid entry defects" { expect_defect(fn() { checked(101) }) }
test "external input can be rejected" {
  expect(Result.is_err(Percent.validate(101)))
}
```

### Replace time with a deterministic fake

Keep the effect in the production function's row and replace its handler in the
test. Query other effects' fake or replay handlers before writing a live test.

```sakura test id=skill-fake-clock
import std/instant.{Instant}
fn observed_time() -> Instant ! {Clock} { Clock.now() }
test "clock is controlled by the test" {
  let at = Instant.from_unix_millis(42)
  let seen = with Clock.fixed(at) { observed_time() }
  expect_eq(seen, at)
}
```

### Recover a typed failure

Catch only the expected tag; other failures must remain in the signature.

```sakura test id=skill-fail-recovery
error Missing
fn lookup() -> Int ! {Fail<Missing>} { fail Missing }
fn fallback() -> Int { try { lookup() } catch { Missing => 0 } }
test "missing value uses the fallback" { expect_eq(fallback(), 0) }
```

