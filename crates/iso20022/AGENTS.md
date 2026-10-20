# veinte-iso20022 — the wire format

**Owns** encoding and decoding of `head.001.001.04`, `pacs.008.001.08` and `pacs.002.001.10`, and the mapping of malformed input to Tabla 12 codes.

**Never depends on** any Veinte crate but `veinte-types`.

**Rules**

- The profile is ours, marked **[assumption]** until the BCE publishes the catálogo técnico. Its element choices follow catálogo §8 where it is explicit; nothing field-level is invented beyond what a message needs to route.
- Every message type has a golden file in `tests/golden`. Regenerate with `UPDATE_GOLDEN=1 cargo test -p veinte-iso20022` and review the diff.
