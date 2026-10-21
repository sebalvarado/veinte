# Veinte

Open tooling for Ecuador's real-time payments standard: the scheme's vocabulary and its ISO 20022 messages, in Rust. Built by the team behind [Veinte](https://veinte.ec).

> Independent project. Not affiliated with, nor endorsed by, the Banco Central del Ecuador.

## What is here

| Crate | What it does |
|---|---|
| [`veinte-types`](crates/types) | The Catálogo Operacional's vocabulary as validated types: amounts in USD, BICs, payment ids (Tabla 14), payment states, the Tabla 12 reason codes, llave types, and the business calendar |
| [`veinte-iso20022`](crates/iso20022) | Encoding and decoding of `head.001.001.04`, `pacs.008.001.08` and `pacs.002.001.10`, with malformed input mapped to Tabla 12 codes |

Coming next, in this repository:

- **A command-line tool.** Validate a message against the profile, explain a reason code in Spanish, and build or check a payment id and a llave.
- **`tck`, the conformance kit.** It impersonates the counterparty, drives your implementation through the catálogo's scenarios, and writes a Spanish report you can hand to your red.

The message profile is ours, marked **[assumption]**, until the BCE publishes the catálogo técnico. Every rule cites the catálogo section it comes from.

## Use it

```bash
cargo test --workspace
```

Requires the Rust toolchain pinned in `rust-toolchain.toml`.

## Contributing

Contributions are welcome under the Developer Certificate of Origin: sign off every commit with `git commit -s`. See [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`SECURITY.md`](SECURITY.md).

## License

[Apache-2.0](LICENSE).
