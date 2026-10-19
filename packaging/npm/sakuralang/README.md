# Sakura Lang

**The programming language built for agents.**

Sakura Lang is an agent-first programming language built around explicit, reviewable promises.
A function's signature states what it can do (its effects), how it can fail (its errors) and what
it guarantees (its contracts). The compiler checks types and effects, checks contracts at runtime
and can prove them with an SMT solver. Humans review signatures; agents get precise, structured
feedback for small changes.

This package installs the `sakura` command: compiler, formatter, test runner, LSP and MCP server.

> **Preview.** This is a pre-1.0 release with no stable language or API contract yet. See
> [STABILITY.md](https://github.com/icantcodefyi/sakuralang/blob/main/STABILITY.md).

## Install

```sh
npm i -g sakuralang
```

`check` and `build` need nothing else; `run` and `test` need Node 26. npm installs only the
matching platform package (`sakuralang-<os>-<cpu>`); nothing runs or downloads at install time.
Builds exist for macOS on Apple silicon and Linux x64 (glibc 2.35+); see the
[install guide](https://www.sakuralang.com/docs/install) for the other platforms, Homebrew and
`install.sh`.

## Quickstart

```sh
sakura new cli hello && cd hello
sakura check            # no problems
sakura run -- Sakura    # hello, Sakura
sakura test             # 1 passed, 0 failed
```

`sakura new` writes `sakura.toml`, `src/main.sak` with one test, and `AGENTS.md`/`CLAUDE.md` for
coding agents. Add `--setup claude` (or `codex`, `cursor`, `all`) to install the Sakura skill and
MCP entry for your agent.

## A taste

```sakura
fn withdraw(acct: Account, amount: Cents)
  -> Account
  ! {Ledger, Fail<Overdrawn | Frozen>}
  requires positive: amount > 0
  ensures result.balance == acct.balance - amount
{
  if Ledger.frozen(acct.id) { fail Frozen }
  if amount > acct.balance { fail Overdrawn { balance: acct.balance } }
  let next = Account { id: acct.id, balance: Cents.from(acct.balance - amount) }
  Ledger.record(next)
  next
}
```

A caller must handle `Ledger` and both errors, or declare them in its own signature.

## Links

- Website and docs: https://www.sakuralang.com
- Source, issues and changelog: https://github.com/icantcodefyi/sakuralang
- For agents: https://www.sakuralang.com/llms.txt

Licensed under MIT OR Apache-2.0.
