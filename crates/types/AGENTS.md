# veinte-types — the kernel

**Owns** the catálogo vocabulary every Veinte domain shares: `Usd`, `Bic`, `PaymentId`, `PaymentState`, `ReasonCode` / `ReceivedReason`, `LlaveType`, and the `BusinessCalendar` with scheme time.

**Never depends on** another Veinte crate, or on anything that does I/O, reads a clock, or draws randomness. `jiff` is the only dependency.

**Rules**

- Private fields, validating constructors: a value that exists is valid. Every rule cites the catálogo table it comes from.
- A catálogo revision that adds a state or a code lands here deliberately, with its citation.
- A change here rebuilds every dependent. Keep it small.
