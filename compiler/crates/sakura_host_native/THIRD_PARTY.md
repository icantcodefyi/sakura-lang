# Native toolchain third-party notices

N0a registry check: 2026-10-18. Exact direct pins are in the workspace manifest.
The table records upstream SPDX choices; linked files are unmodified upstream notices.
The native HTTP stack selects ring (reqwest defaults and system proxy are disabled);
SQLite is bundled. The native runtime is experimental and off the tag gates (design
§17.245 (2)), and `release.yml` ships no native binary; this file is what a binary
distribution ships beside one. Rows cover every dependency of `sakura_host_native` and of
`sakura_rt` (its `native` feature included), and every transitive crate whose licence
requires its notice in a binary (BSD-3-Clause `curve25519-dalek`, design §17.288 (9));
`tests/third_party.rs` keeps the direct rows complete. Other transitive crates remain
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
| [miniz_oxide](https://crates.io/crates/miniz_oxide/0.9.1) | 0.9.1 | MIT OR Zlib OR Apache-2.0; linked into `sakura_rt` (native and the WASM runtime) for `std/compress` | [LICENSE-ZLIB](third-party/miniz_oxide/LICENSE-ZLIB.md), [LICENSE-MIT](third-party/miniz_oxide/LICENSE-MIT.md) |
| [adler2](https://crates.io/crates/adler2/2.0.1) | 2.0.1 | 0BSD OR MIT OR Apache-2.0; miniz_oxide's dependency | [LICENSE-0BSD](third-party/adler2/LICENSE-0BSD), [LICENSE-MIT](third-party/adler2/LICENSE-MIT) |
| [serde_json](https://crates.io/crates/serde_json/1.0.154) | 1.0.154 | MIT OR Apache-2.0 | [LICENSE-APACHE](third-party/serde_json/LICENSE-APACHE), [LICENSE-MIT](third-party/serde_json/LICENSE-MIT) |
| [libc](https://crates.io/crates/libc/0.2.191) | 0.2.191 | MIT OR Apache-2.0; `sakura_rt`'s `native` feature | [LICENSE-APACHE](third-party/libc/LICENSE-APACHE), [LICENSE-MIT](third-party/libc/LICENSE-MIT) |
| [sha2](https://crates.io/crates/sha2/0.11.1) | 0.11.1 | MIT OR Apache-2.0; `std/crypto` on native (`sakura_rt`'s `native` feature) | [LICENSE-APACHE](third-party/sha2/LICENSE-APACHE), [LICENSE-MIT](third-party/sha2/LICENSE-MIT) |
| [hmac](https://crates.io/crates/hmac/0.13.0) | 0.13.0 | MIT OR Apache-2.0; `std/crypto` on native | [LICENSE-APACHE](third-party/hmac/LICENSE-APACHE), [LICENSE-MIT](third-party/hmac/LICENSE-MIT) |
| [hkdf](https://crates.io/crates/hkdf/0.13.0) | 0.13.0 | MIT OR Apache-2.0; `std/crypto` on native | [LICENSE-APACHE](third-party/hkdf/LICENSE-APACHE), [LICENSE-MIT](third-party/hkdf/LICENSE-MIT) |
| [pbkdf2](https://crates.io/crates/pbkdf2/0.13.0) | 0.13.0 | MIT OR Apache-2.0; `std/crypto` on native | [LICENSE-APACHE](third-party/pbkdf2/LICENSE-APACHE), [LICENSE-MIT](third-party/pbkdf2/LICENSE-MIT) |
| [aes-gcm](https://crates.io/crates/aes-gcm/0.11.1) | 0.11.1 | Apache-2.0 OR MIT; `std/crypto` on native | [LICENSE-APACHE](third-party/aes-gcm/LICENSE-APACHE), [LICENSE-MIT](third-party/aes-gcm/LICENSE-MIT) |
| [p256](https://crates.io/crates/p256/0.14.0) | 0.14.0 | Apache-2.0 OR MIT; `std/crypto` on native | [LICENSE-APACHE](third-party/p256/LICENSE-APACHE), [LICENSE-MIT](third-party/p256/LICENSE-MIT) |
| [ed25519-dalek](https://crates.io/crates/ed25519-dalek/3.0.0) | 3.0.0 | BSD-3-Clause; `std/crypto` on native | [LICENSE](third-party/ed25519-dalek/LICENSE) |
| [x25519-dalek](https://crates.io/crates/x25519-dalek/3.0.0) | 3.0.0 | BSD-3-Clause; `std/crypto` on native | [LICENSE](third-party/x25519-dalek/LICENSE) |
| [curve25519-dalek](https://crates.io/crates/curve25519-dalek/5.0.0) | 5.0.0 | BSD-3-Clause; transitive through `ed25519-dalek` and `x25519-dalek` | [LICENSE](third-party/curve25519-dalek/LICENSE) |
| [zeroize](https://crates.io/crates/zeroize/1.9.1) | 1.9.1 | Apache-2.0 OR MIT; `rt::crypto`'s zeroed copies | [LICENSE-APACHE](third-party/zeroize/LICENSE-APACHE), [LICENSE-MIT](third-party/zeroize/LICENSE-MIT) |
