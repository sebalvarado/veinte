use std::fmt;
use std::str::FromStr;

use crate::InvalidValue;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Usd(u64);

impl Usd {
    pub const ZERO: Self = Self(0);

    pub const fn from_cents(cents: u64) -> Self {
        Self(cents)
    }

    pub const fn cents(self) -> u64 {
        self.0
    }

    pub const fn checked_add(self, other: Self) -> Option<Self> {
        match self.0.checked_add(other.0) {
            Some(c) => Some(Self(c)),
            None => None,
        }
    }

    pub const fn checked_sub(self, other: Self) -> Option<Self> {
        match self.0.checked_sub(other.0) {
            Some(c) => Some(Self(c)),
            None => None,
        }
    }
}

impl FromStr for Usd {
    type Err = InvalidValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        const ERR: InvalidValue = InvalidValue::new("Usd", "digits with at most two decimals");
        let (whole, frac) = s.split_once('.').unwrap_or((s, "00"));
        let digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
        if !digits(whole) || !digits(frac) || frac.len() > 2 {
            return Err(ERR);
        }
        let frac = if frac.len() == 1 { frac.parse::<u64>().map(|d| d * 10) } else { frac.parse() };
        whole
            .parse::<u64>()
            .ok()
            .and_then(|w| w.checked_mul(100))
            .zip(frac.ok())
            .and_then(|(w, f)| w.checked_add(f))
            .map(Self)
            .ok_or(ERR)
    }
}

impl fmt::Display for Usd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:02}", self.0 / 100, self.0 % 100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn parses_the_worked_example() {
        assert_eq!("100.29".parse(), Ok(Usd::from_cents(10_029)));
        assert_eq!("0.04".parse(), Ok(Usd::from_cents(4)));
        assert_eq!("100".parse(), Ok(Usd::from_cents(10_000)));
        assert_eq!("0.5".parse(), Ok(Usd::from_cents(50)));
        assert_eq!("0".parse(), Ok(Usd::ZERO));
    }

    #[test]
    fn refuses_sub_cent_and_malformed_amounts() {
        for s in ["0.0375", "", ".5", "5.", "-1", "+1", "1,00", "1e2", " 1", "18446744073709551616"] {
            assert!(s.parse::<Usd>().is_err(), "{s:?} parsed");
        }
    }

    #[test]
    fn checked_arithmetic_never_wraps() {
        assert_eq!(Usd::from_cents(1).checked_sub(Usd::from_cents(2)), None);
        assert_eq!(Usd::from_cents(u64::MAX).checked_add(Usd::from_cents(1)), None);
    }

    proptest! {
        #[test]
        fn display_round_trips(cents: u64) {
            let usd = Usd::from_cents(cents);
            prop_assert_eq!(usd.to_string().parse(), Ok(usd));
        }
    }
}
