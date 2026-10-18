# Native toolchain third-party notices

N0a registry check: 2026-10-18. Exact direct pins are in the workspace manifest.
The table records upstream SPDX choices; linked files are unmodified upstream notices.
The native HTTP stack selects ring (reqwest defaults and system proxy are disabled);
SQLite is bundled. This skeleton ships no release artifact. Transitive crates remain
locked in `compiler/Cargo.lock`; full source archives retain their own licence files.

| Crate | Version | Upstream license | Notices |
|---|---|---|---|
| [cranelift-codegen](https://crates.io/crates/cranelift-codegen/0.136.3) | 0.136.3 | Apache-2.0 WITH LLVM-exception | [LICENSE](third-party/cap-std/LICENSE-Apache-2.0_WITH_LLVM-exception) |
| [cranelift-frontend](https://crates.io/crates/cranelift-frontend/0.136.3) | 0.136.3 | Apache-2.0 WITH LLVM-exception | [LICENSE](third-party/cap-std/LICENSE-Apache-2.0_WITH_LLVM-exception) |
| [cranelift-module](https://crates.io/crates/cranelift-module/0.136.3) | 0.136.3 | Apache-2.0 WITH LLVM-exception | [LICENSE](third-party/cap-std/LICENSE-Apache-2.0_WITH_LLVM-exception) |
| [cranelift-object](https://crates.io/crates/cranelift-object/0.136.3) | 0.136.3 | Apache-2.0 WITH LLVM-exception | [LICENSE](third-party/cap-std/LICENSE-Apache-2.0_WITH_LLVM-exception) |
| [cranelift-native](https://crates.io/crates/cranelift-native/0.136.3) | 0.136.3 | Apache-2.0 WITH LLVM-exception | [LICENSE](third-party/cap-std/LICENSE-Apache-2.0_WITH_LLVM-exception) |
| [target-lexicon](https://crates.io/crates/target-lexicon/0.13.5) | 0.13.5 | Apache-2.0 WITH LLVM-exception | [LICENSE](third-party/cap-std/LICENSE-Apache-2.0_WITH_LLVM-exception) |
| [gimli](https://crates.io/crates/gimli/0.34.0) | 0.34.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](third-party/cap-std/LICENSE-APACHE), [LICENSE-MIT](third-party/gimli/LICENSE-MIT) |
| [object](https://crates.io/crates/object/0.40.0) | 0.40.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](third-party/cap-std/LICENSE-APACHE), [LICENSE-MIT](third-party/object/LICENSE-MIT) |
| [tokio](https://crates.io/crates/tokio/1.53.3) | 1.53.3 | MIT | [LICENSE](third-party/tokio/LICENSE) |
| [reqwest](https://crates.io/crates/reqwest/0.13.5) | 0.13.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](third-party/reqwest/LICENSE-APACHE), [LICENSE-MIT](third-party/reqwest/LICENSE-MIT) |
| [rustls](https://crates.io/crates/rustls/0.23.46) | 0.23.46 | Apache-2.0 OR ISC OR MIT | [LICENSE-APACHE](third-party/cap-std/LICENSE-APACHE), [LICENSE-ISC](third-party/rustls/LICENSE-ISC), [LICENSE-MIT](third-party/rustls/LICENSE-MIT) |
| [rusqlite](https://crates.io/crates/rusqlite/0.40.2) | 0.40.2 | MIT | [LICENSE](third-party/libsqlite3-sys/LICENSE) |
| [hyper](https://crates.io/crates/hyper/1.12.0) | 1.12.0 | MIT | [LICENSE](third-party/hyper/LICENSE) |
| [cap-std](https://crates.io/crates/cap-std/4.0.3) | 4.0.3 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [LICENSE-APACHE](third-party/cap-std/LICENSE-APACHE), [LICENSE-Apache-2.0_WITH_LLVM-exception](third-party/cap-std/LICENSE-Apache-2.0_WITH_LLVM-exception), [LICENSE-MIT](third-party/cap-std/LICENSE-MIT) |
| [crabgrind](https://crates.io/crates/crabgrind/0.4.0) | 0.4.0 | MIT; bundled Valgrind headers: BSD-style | [MIT](third-party/crabgrind/MIT.LICENSE), [header notices](third-party/crabgrind/) |
| [ring](https://crates.io/crates/ring/0.17.14) | 0.17.14 | Apache-2.0 AND ISC | [LICENSE](third-party/ring/LICENSE), [LICENSE-BoringSSL](third-party/ring/LICENSE-BoringSSL), [LICENSE-other-bits](third-party/ring/LICENSE-other-bits) |
| [libsqlite3-sys](https://crates.io/crates/libsqlite3-sys/0.38.2) | 0.38.2 | MIT | [LICENSE](third-party/libsqlite3-sys/LICENSE) |
