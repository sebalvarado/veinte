# AGENTS.md

Open tooling for Ecuador's real-time payments standard, Apache-2.0. Independent; **not affiliated with the BCE** — the disclaimer stays on every public artifact.

- `cargo test --workspace` is the loop; `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` must pass before a pull request.
- Each crate's `AGENTS.md` says what it owns and what it may depend on.
- Every catálogo rule cites its section of the Catálogo Operacional; anything else is marked **[assumption]**. The catálogo técnico is unpublished, so field-level rules are never invented.
- No credentials, keys or real account data, ever, even as fixtures.
- Commits: DCO sign-off (`-s`) and an imperative subject. English for code, identifiers and commits.
