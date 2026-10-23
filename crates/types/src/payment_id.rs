use std::fmt;
use std::str::FromStr;

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

/// Catálogo Operacional §7, Tabla 14: the ordering BIC, then 14 digits `DDMMAAAAHHMMSS`, then a
/// serial or hash. The whole identifier is 20 to 35 printable ASCII characters.
///
/// [assumption] Tabla 14 does not fix the BIC length. Parsing succeeds only when exactly one
/// prefix of 1 to 11 characters is a BIC that [`PaymentId::new`] accepts for the whole string.
impl FromStr for PaymentId {
    type Err = InvalidValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut found = None;
        for len in 1..=s.len().min(11) {
            if !s.is_char_boundary(len) {
                continue;
            }
            let Ok(ordering) = s[..len].parse::<Bic>() else { continue };
            let Ok(id) = Self::new(ordering, s) else { continue };
            if found.is_some() {
                return Err(InvalidValue::new(
                    "PaymentId",
                    "20 to 35 printable ASCII characters: the ordering BIC, then 14 digits DDMMAAAAHHMMSS, with only one possible BIC",
                ));
            }
            found = Some(id);
        }
        found.ok_or(InvalidValue::new(
            "PaymentId",
            "20 to 35 printable ASCII characters: the ordering BIC, then 14 digits DDMMAAAAHHMMSS",
        ))
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
    fn parses_the_tabla_14_string() {
        // Catálogo Operacional §7, Tabla 14: ordering BIC, DDMMAAAAHHMMSS, then a serial or hash.
        let id: PaymentId = "BALFECEQ10102026093000A1B2C3D4E5F6G".parse().unwrap();
        assert_eq!(id, PaymentId::new(bic("BALFECEQ"), "BALFECEQ10102026093000A1B2C3D4E5F6G").unwrap());
        assert_eq!(id.to_string().parse::<PaymentId>().unwrap(), id);
    }

    #[test]
    fn parses_an_identifier_with_no_tail() {
        let id: PaymentId = "BALFECEQ10102026093000".parse().unwrap();
        assert_eq!(id.ordering(), bic("BALFECEQ"));
        assert_eq!(id.as_str().len(), 22);
    }

    #[test]
    fn refuses_an_ambiguous_bic_split() {
        // [assumption] Tabla 14 does not fix the BIC length. A digit tail can be read as a longer BIC.
        assert!("AB1010202609300012XYZ".parse::<PaymentId>().is_err());
    }

    #[test]
    fn refuses_a_string_that_is_not_an_identifier() {
        for s in ["", "BALFECEQ", "not-a-payment-id", "BALFECEQ1010202609300", "é10102026093000123456"] {
            assert!(s.parse::<PaymentId>().is_err(), "{s:?} parsed");
        }
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

        #[test]
        fn a_letter_bic_round_trips_through_from_str(b in "[A-Z]{8}", stamp in "[0-9]{14}", tail in "[A-Z]{0,13}") {
            let code = format!("{b}{stamp}{tail}");
            let id = PaymentId::new(bic(&b), &code).unwrap();
            prop_assert_eq!(code.parse::<PaymentId>().unwrap(), id);
        }

        // Catálogo Operacional §7, Tabla 14: the third column is a serial or a hash, so a digit tail is normal.
        #[test]
        fn a_numeric_serial_keeps_the_original_bic_or_fails(
            b in "([A-Z0-9]{8}|[A-Z0-9]{11})",
            day in 1u8..=28,
            month in 1u8..=12,
            year in 2020u16..=2030,
            hour in 0u8..=23,
            minute in 0u8..=59,
            second in 0u8..=59,
            tail in "[0-9]{0,13}",
        ) {
            let stamp = format!("{day:02}{month:02}{year:04}{hour:02}{minute:02}{second:02}");
            let code = format!("{b}{stamp}{tail}");
            prop_assume!((20..=35).contains(&code.len()));
            let built = PaymentId::new(bic(&b), &code).unwrap();
            // An ambiguous digit tail is an error. A success has to be this BIC.
            if let Ok(parsed) = code.parse::<PaymentId>() {
                prop_assert_eq!(parsed, built);
            }
        }
    }
}
