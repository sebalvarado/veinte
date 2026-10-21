# Contributing

Thank you for helping. Issues and pull requests are welcome in Spanish or English.

## Rules

1. **Sign off every commit** with `git commit -s` (the [Developer Certificate of Origin](https://developercertificate.org/)). CI checks it.
2. **Cite the catálogo.** A state, code, llave type, timing or message rule cites the section of the BCE's Catálogo Operacional it comes from. Anything the catálogo does not define is marked **[assumption]**.
3. **Write the failing test first.** Message changes come with a golden file in `crates/iso20022/tests/golden`. Type rules come with property tests.
4. **No real data.** No credentials, keys or real account data, ever, even as fixtures.

## Before opening a pull request

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
