# GitHub Linguist submission (prepared, not submitted)

Everything needed to add Sakura Lang (`.sak`) to [GitHub Linguist](https://github.com/github-linguist/linguist),
the library behind GitHub's language bar and code highlighting. Nothing here has been sent upstream: no
fork, PR or issue exists. **Do not submit yet**: see [Usage](#usage-against-the-threshold). Measured and
dry-run on 2026-10-22 against linguist `main` and this repository at `377f7e9c`.

## Linguist's requirements

From linguist's [CONTRIBUTING.md](https://github.com/github-linguist/linguist/blob/main/CONTRIBUTING.md)
("Adding a language", "Language extension and filename usage requirements") and the field reference at the
top of [`lib/linguist/languages.yml`](https://github.com/github-linguist/linguist/blob/main/lib/linguist/languages.yml):

- **Usage.** "We do not accept PRs for very new or hobby languages." The bar is GitHub code search, excluding
  forks, indexed in the last year: **at least 2,000 files** for an extension that occurs more than once per
  repository (`.sak` does), with "a reasonable distribution across unique `:user/:repo` combinations". Results
  dominated by the language's owner are filtered out with `-user:<username>`.
- **Grammar.** A TextMate grammar added as a git submodule with `script/add-grammar <url>`, under one of the
  [allowed licences](https://github.com/github-linguist/linguist/blob/main/vendor/licenses/config.yml):
  apache-2.0, bsd-2/3-clause, cc0-1.0, isc, mit, mpl-2.0, ncsa, unlicense, wtfpl, zlib. Linguist compiles it with
  PCRE, not Oniguruma. Tree-sitter grammars are not used.
- **Samples.** Real-world code under `samples/<Language>/`; "'Hello world' and other examples found in tutorials
  will not be accepted". State the licence (or that the sample may be used under Linguist's MIT licence).
- **`languages.yml` fields.** Required: `type`, `ace_mode` (`text` when Ace has no mode), `extensions`,
  `tm_scope`, and `language_id`, which `script/update-ids` generates (never hand-written). Optional:
  `color` (`#RRGGBB`, strongly recommended; `#cccccc` otherwise), `aliases`, `codemirror_mode` and
  `codemirror_mime_type` (only for an existing CodeMirror 5 mode; Sakura has none, so both are omitted).
- **Heuristics** only when the extension is already claimed. `.sak` is **not** in `languages.yml` or
  `heuristics.yml` today, so none is needed. One unlisted project in the wild also uses `.sak`
  ([Sakum Lang](https://github.com/minaakshigosain/Sakum)); if it is ever listed first, add two samples
  per language and a heuristic (Sakura: `import std/…`, `! {Effect}` rows, `@total`).
- **The PR template** ([`.github/PULL_REQUEST_TEMPLATE.md`](https://github.com/github-linguist/linguist/blob/main/.github/PULL_REQUEST_TEMPLATE.md))
  must be filled in or the PR is not reviewed.

## Usage against the threshold

GitHub REST code search (`GET /search/code`, the free API; the web UI's `NOT is:fork path:*.sak` count needs a
signed-in browser and may differ), 2026-10-22:

| Query | Files | Notes |
|---|---:|---|
| `extension:sak` | 934 | everything named `.sak` |
| `extension:sak -user:icantcodefyi` | 164 | 17 repos; none is Sakura Lang: Sakum Lang, SAKO, traffic-signal data, `gfastats` output, a files-for-every-extension list |
| `extension:sak fn -user:icantcodefyi` | 6 | Sakum Lang and one unrelated repo |
| `extension:sak "import std" -user:icantcodefyi` | 1 | Sakum Lang |

Sakura Lang files outside this repository: **0** against a bar of **2,000** spread across many users. This
repository tracks 923 `.sak` files, but owner results are filtered out. A PR today would be closed under
"very new or hobby languages". Re-measure before submitting:

```sh
gh api -X GET search/code -f q='extension:sak "import std/" -user:icantcodefyi' --jq .total_count
```

and in a browser: <https://github.com/search?type=code&q=NOT+is%3Afork+path%3A*.sak+%22import+std%2F%22+-user%3Aicantcodefyi>.

## What is ready in this repository

- **Grammar:** [`editors/vscode/syntaxes/sakura.tmLanguage.json`](../editors/vscode/syntaxes/sakura.tmLanguage.json),
  scope `source.sakura`, generated from `website/lib/sakura/grammar.ts` (`bun run grammar` in `editors/vscode/`;
  its `bun run test` fails on drift). It is the only grammar file in the repository, and it sits in a `syntaxes/`
  directory, which is where Linguist's grammar compiler looks. Licence MIT OR Apache-2.0 (root
  `LICENSE-MIT`, `LICENSE-APACHE`; GitHub detects apache-2.0). Both are on the allow-list.
- **Checks:** `bun scripts/check-grammar.ts` in `website/` asserts key scopes and tokenizes every tracked
  `.sak` file (923 files, 41,668 lines): no `invalid` scope except reserved words, no file over 1 s.
  Linguist's own compiler (`script/grammar-compiler add`, Docker image `linguist/grammar-compiler`) accepted it
  with no errors or warnings: `OK! added grammar source 'vendor/grammars/sakuralang'`, `new scope: source.sakura`.
- **Dry run** in a local, unpushed linguist clone (Docker `ruby:3.4`): with the entry and samples below,
  `script/update-ids` assigned `language_id: 923489771` (the owner's run regenerates it), all three samples
  detect as `Sakura Lang`, `test/test_samples.rb` passes (1678 runs, 0 failures), and `test_language.rb`'s only
  failures were the CodeMirror checks for other languages, because the shallow clone had no `vendor/CodeMirror`
  submodule.
- **Colour:** `#f386a1`, the site's `--sakura` token (`website/app/globals.css`, `website/design/BRAND.md`).
- **`.gitattributes`:** `*.sak linguist-language=Sakura-Lang`, the documented override
  ([docs/overrides.md](https://github.com/github-linguist/linguist/blob/main/docs/overrides.md); spaces become
  hyphens). Linguist ignores names it does not list, so it does nothing until the language is merged.

## The `languages.yml` entry

Between `Sail:` and `Salt:` (alphabetical). `language_id` is left out; `script/update-ids` adds it.

```yaml
Sakura Lang:
  type: programming
  color: "#f386a1"
  aliases:
  - sakura
  extensions:
  - ".sak"
  tm_scope: source.sakura
  ace_mode: text
```

`sakura lang` is an implicit alias; `sakura` makes ` ```sakura ` fences highlight.

## Samples

Copy into `samples/Sakura Lang/` (real programs, not tutorials; MIT OR Apache-2.0, and the owner may also state
they are offered under Linguist's MIT licence):

| Sample | Source | What it shows |
|---|---|---|
| `api.sak` | [`dogfood/repair-queue/src/api.sak`](../dogfood/repair-queue/src/api.sak) | imports, records, HTTP handlers, error mapping |
| `policy.sak` | [`dogfood/parcel-quotes/src/policy.sak`](../dogfood/parcel-quotes/src/policy.sak) | refinement types (`where`), doc comments, `Fail` |
| `list.sak` | [`compiler/std/list.sak`](../compiler/std/list.sak) | generics, `@external`/`@total`, `requires` contracts |

Link them in the PR at a commit SHA (`https://github.com/icantcodefyi/sakuralang/blob/<sha>/<path>`).

## Owner steps

Prerequisites: Docker running (grammar compiler), and Ruby with `cmake`, `pkg-config` and ICU, or use the
linguist dev container / Codespace, which has them.

```sh
# 1. Fork github-linguist/linguist on GitHub, then:
git clone https://github.com/<you>/linguist.git && cd linguist
script/bootstrap
git switch -c add-sakura-lang

# 2. The languages.yml entry above, between Sail: and Salt:.
$EDITOR lib/linguist/languages.yml

# 3. The grammar submodule (the whole repository; the grammar is editors/vscode/syntaxes/).
script/add-grammar https://github.com/icantcodefyi/sakuralang

# 4. Samples.
mkdir -p "samples/Sakura Lang"
for f in dogfood/repair-queue/src/api.sak dogfood/parcel-quotes/src/policy.sak compiler/std/list.sak; do
  curl -fsSL "https://raw.githubusercontent.com/icantcodefyi/sakuralang/<sha>/$f" -o "samples/Sakura Lang/${f##*/}"
done

# 5. The language_id, then tests.
script/update-ids
bundle exec rake samples
bundle exec rake test
bundle exec bin/github-linguist "samples/Sakura Lang/api.sak"   # expect: Sakura Lang

# 6. Commit, push to the fork, open the PR with the body below.
git add -A && git commit -m "Add Sakura Lang"
```

`script/add-grammar` registers `vendor/grammars/sakuralang`, adds `source.sakura` to `grammars.yml`, caches the
licence under `vendor/licenses/` and updates `vendor/README.md`. Linguist tracks the submodule's default branch,
so later grammar changes reach GitHub with Linguist's releases, not ours.

A dedicated grammar repository (only the `.tmLanguage.json` and licences) would make the submodule small; it
needs syncing from `website/lib/sakura/grammar.ts` on every grammar change, so it is not set up.

## Draft PR body

Title: `Add Sakura Lang`

```markdown
## Description

Adds [Sakura Lang](https://www.sakuralang.com), an agent-first, statically typed language with effect rows
and contracts that compiles to JavaScript and WebAssembly. Extension `.sak` (not currently claimed in
languages.yml).

## Checklist:

- [x] **I am adding a new language.**
  - [x] The extension of the new language is used in hundreds of repositories on GitHub.com.
    - Search results for each extension:
      - https://github.com/search?type=code&q=NOT+is%3Afork+path%3A*.sak+%22import+std%2F%22+-user%3Aicantcodefyi
  - [x] I have included a real-world usage sample for all extensions added in this PR:
    - Sample source(s):
      - https://github.com/icantcodefyi/sakuralang/blob/<sha>/dogfood/repair-queue/src/api.sak
      - https://github.com/icantcodefyi/sakuralang/blob/<sha>/dogfood/parcel-quotes/src/policy.sak
      - https://github.com/icantcodefyi/sakuralang/blob/<sha>/compiler/std/list.sak
    - Sample license(s): MIT OR Apache-2.0 (I am the author and am also happy for them to be included under
      Linguist's MIT licence).
  - [x] I have included a syntax highlighting grammar: https://github.com/icantcodefyi/sakuralang
    (`editors/vscode/syntaxes/sakura.tmLanguage.json`, MIT OR Apache-2.0)
  - [x] I have added a color
    - Hex value: `#f386a1`
    - Rationale: the sakura pink of the language's brand (`--sakura` in the website theme).
  - [ ] I have updated the heuristics to distinguish my language from others using the same extension.
    (Not needed: `.sak` is not used by any language in languages.yml.)
```

Only tick the usage box once the search link shows the required count.
