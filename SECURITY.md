# Security policy

**Do not open a public issue for a vulnerability.** [Report it privately on GitHub](https://github.com/sebalvarado/veinte/security/advisories/new). Reports in Spanish or English are equally welcome.

Include the affected crate and commit, how to reproduce it, and the impact if you know it. We acknowledge within 72 hours, assess within 7 days, and coordinate disclosure with you; the default embargo is 90 days or until a fix ships, whichever comes first.

We will not pursue legal action for good-faith research that stays away from other people's data, does not disrupt third parties, and is reported privately as above.

Especially valuable: message parsing and validation in `veinte-iso20022`, and any validating constructor in `veinte-types` that accepts a value it should refuse.
