# caudal-types — the kernel

**Owns** the catálogo vocabulary every domain shares ([ADR-0014](../../docs/architecture/adr/0014-kernel-types.md)): `Usd`, `Bic`, `PaymentId`, `PaymentState`, `ReasonCode` / `ReceivedReason`, `LlaveType`, and the `BusinessCalendar` with scheme time ([ADR-0017](../../docs/architecture/adr/0017-time-calendar-identifiers.md)).

**Never depends on** another Caudal crate, or on anything that does I/O, reads a clock, or draws randomness. `jiff` is the only dependency.

**Rules**

- Private fields, validating constructors: a value that exists is valid. Every rule cites the catálogo table it comes from (`docs/research/catalogo-operacional.md`, verified against `research/bce/`).
- Adding a type or a variant is an ADR change. A catálogo revision that adds a state or a code lands here deliberately.
- A change here rebuilds the workspace. Keep it small.
