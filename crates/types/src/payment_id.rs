use std::fmt;

use crate::{Bic, InvalidValue};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PaymentId {
    ordering: Bic,
    code: Box<str>,
}

impl PaymentId {
    pub fn new(ordering: Bic, code: &str) -> Result<Self, InvalidValue> {
        let rest = code.strip_prefix(ordering.as_str());
        let valid = (20..=35).contains(&code.len())
            && code.bytes().all(|b| b.is_ascii_graphic())
            && rest.is_some_and(|r| r.len() >= 14 && r.bytes().take(14).all(|b| b.is_ascii_digit()));
        if !valid {
            return Err(InvalidValue::new(
                "PaymentId",
                "20 to 35 printable ASCII characters: the ordering BIC, then 14 digits DDMMAAAAHHMMSS",
            ));
        }
        Ok(Self { ordering, code: code.into() })
    }

    pub fn ordering(&self) -> Bic {
        self.ordering
    }

    pub fn as_str(&self) -> &str {
        &self.code
    }
}

impl fmt::Display for PaymentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.code)
    }
}

impl fmt::Debug for PaymentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PaymentId({})", self.code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn bic(s: &str) -> Bic {
        s.parse().unwrap()
    }

    #[test]
    fn accepts_the_tabla_14_shape() {
        let id = PaymentId::new(bic("BALFECEQ"), "BALFECEQ10102026093000A1B2C3D4E5F6G").unwrap();
        assert_eq!(id.ordering(), bic("BALFECEQ"));
        assert_eq!(id.as_str().len(), 35);
    }

    #[test]
    fn refuses_a_code_whose_bic_disagrees() {
        assert!(PaymentId::new(bic("CBETECEQ"), "BALFECEQ10102026093000A1B2C3").is_err());
    }

    #[test]
    fn refuses_bad_lengths_and_dates() {
        let b = bic("BALFECEQ");
        for code in [
            "BALFECEQ1010202609",
            "BALFECEQ10102026093000A1B2C3D4E5F6G7",
            "BALFECEQ1010202609300XA1B2C3",
            "BALFECEQ10102026093000 A1B2C3",
        ] {
            assert!(PaymentId::new(b, code).is_err(), "{code:?} accepted");
        }
    }

    proptest! {
        #[test]
        fn accepts_every_well_formed_code(b in "[A-Z0-9]{1,11}", stamp in "[0-9]{14}", tail in "[A-Z2-7]{0,20}") {
            let code: String = format!("{b}{stamp}{tail}").chars().take(35).collect();
            prop_assume!(code.len() >= 20);
            let id = PaymentId::new(bic(&b), &code).unwrap();
            prop_assert_eq!(id.as_str(), code.as_str());
        }
    }
}
