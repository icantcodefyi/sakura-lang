# A `file://` registry fixture

An index in RFC 0003 §6.2's layout for `sakura_packages`' tests (`tests/resolve.rs`). The
checksums are placeholders: no tarball is fetched from it. `backoff 0.4.2` is yanked, so a
`0.4.2` floor resolves to `0.4.3`.
