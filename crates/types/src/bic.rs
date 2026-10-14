use std::fmt;
use std::str::FromStr;

use crate::InvalidValue;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Bic {
    len: u8,
    bytes: [u8; 11],
}

impl Bic {
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)]).expect("validated as ASCII")
    }
}

impl FromStr for Bic {
    type Err = InvalidValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let valid = (1..=11).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
        if !valid {
            return Err(InvalidValue::new("Bic", "1 to 11 uppercase ASCII letters or digits"));
        }
        let mut bytes = [0; 11];
        bytes[..s.len()].copy_from_slice(s.as_bytes());
        Ok(Self { len: s.len() as u8, bytes })
    }
}

impl fmt::Display for Bic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Bic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bic({})", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn accepts_short_and_long_codes() {
        for s in ["BALFECEQ", "CBETECEQXXX", "B1"] {
            assert_eq!(s.parse::<Bic>().unwrap().as_str(), s);
        }
    }

    #[test]
    fn refuses_invalid_codes() {
        for s in ["", "balfeceq", "BALF-CEQ", "ABCDEFGHIJKL", "BALFÉCEQ"] {
            assert!(s.parse::<Bic>().is_err(), "{s:?} parsed");
        }
    }

    proptest! {
        #[test]
        fn round_trips(s in "[A-Z0-9]{1,11}") {
            prop_assert_eq!(s.parse::<Bic>().unwrap().to_string(), s);
        }
    }
}
