# veinte-tck — the conformance kit

**Owns** the scenarios that drive a connector, built only from the open crates, through the link profile: a valid exchange of each message, a refused signature, a refused correlation, a timeout on each stage, and each Tabla 12 code the connector may send. It writes the Spanish report.

**May depend on** `veinte-types` and `veinte-iso20022`. Nothing else.

**Rules**

- The reference connector lives here. It is not a red: it shows the profile a third party implements.
- Fixtures stay synthetic. No credentials, keys, or real account data. A scenario generates its keys when it runs.
- The report is Spanish and keeps the non-affiliation disclaimer.
- Independent of the BCE.

Independent project. Not affiliated with, nor endorsed by, the Banco Central del Ecuador.
