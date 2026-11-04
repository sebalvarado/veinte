# veinte-tck — the conformance kit

**Owns** the scenarios that drive an integrante, built only from the open crates, through the link profile: a valid exchange of each message, a refused signature, a refused correlation, the time limit of each stage, and each Tabla 12 code the integrante beneficiario may send. It writes the Spanish report. The scenarios call a trait. The reference integrante is one implementor.

**May depend on** `veinte-types` and `veinte-iso20022`. Nothing else.

**Rules**

- The reference integrante lives here. It is not a red: it shows the profile a third party implements.
- Fixtures stay synthetic. No credentials, keys, or real account data. A scenario generates its keys when it runs.
- The report is Spanish and keeps the non-affiliation disclaimer.
- Independent of the BCE.

Independent project. Not affiliated with, nor endorsed by, the Banco Central del Ecuador.
