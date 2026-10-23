# veinte-cli — the command

**Owns** the `veinte` binary: `validar`, `formatear`, `explicar`, `id`, and `llave`.

**May depend on** `veinte-types`, `veinte-iso20022`, and `clap`. Nothing else.

**Rules**

- Output is Spanish, errors included. Catalog wording comes from `veinte-types`. Library `Display` stays English.
- A `DecodeError` prints `mensaje rechazado: <código> — <description()>` and `detalle: <ruta>`. An `InvalidValue` is a Spanish sentence for its `kind` (`PaymentId`, `PaymentDate`, `Bic`, `LlaveType`) that names the catálogo table.
- `validar` prints the first `DecodeError` and exits 1. Collecting every error waits on the decoder.
- Fixtures stay synthetic. No credentials, keys, or real account data.
- Independent of the BCE. The non-affiliation disclaimer stays in the help text.
