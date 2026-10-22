# veinte-cli — the command

**Owns** the `veinte` binary: `validar`, `formatear`, `explicar`, `id`, and `llave`.

**May depend on** `veinte-types`, `veinte-iso20022`, and `clap`. Nothing else.

**Rules**

- Output is Spanish. Catalog wording comes from `veinte-types`.
- `validar` prints the first `DecodeError` and exits 1. Collecting every error waits on the decoder.
- Fixtures stay synthetic. No credentials, keys, or real account data.
- Independent of the BCE. The non-affiliation disclaimer stays in the help text.
