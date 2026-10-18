# Bundle provenance

The CLI fills the skill's metadata from its version and build-time
`SAKURA_GIT_COMMIT`. Website copies retain placeholders; their content manifest records
the compiler version and file/ZIP SHA-256 hashes, without a Git commit. If provenance
differs, use the installed compiler's help and guide before assuming an API exists.
