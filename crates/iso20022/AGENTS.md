# veinte-iso20022 — the wire format

**Owns** encoding and decoding of `head.001.001.04`, `pacs.008.001.08` and `pacs.002.001.10`, the mapping of malformed input to Tabla 12 codes, and the link profile an integrante speaks with its red: the signature, the correlation between a confirmation and its order, the time limit of each stage, and which Tabla 12 codes the integrante may send.

**Never depends on** any Veinte crate but `veinte-types`.

**Rules**

- The profile is ours, marked **[assumption]** until the BCE publishes the catálogo técnico. Its element choices follow catálogo §8 where it is explicit; nothing field-level is invented beyond what a message needs to route.
- Llave messages named in §11.1 (`acmt.998`) are between a red and the SIP. Their fields are unpublished, so this link does not carry them. `decode` refuses them with `CH16`.
- Every message type has a golden file in `tests/golden`. Regenerate with `UPDATE_GOLDEN=1 cargo test -p veinte-iso20022` and review the diff.
- Independent of the BCE. The non-affiliation disclaimer stays on the crate docs.

Independent project. Not affiliated with, nor endorsed by, the Banco Central del Ecuador.
