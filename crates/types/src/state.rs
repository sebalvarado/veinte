use std::fmt;
use std::str::FromStr;

use crate::InvalidValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaymentState {
    Rcvd,
    Actc,
    Acsp,
    Acsc,
    Rjct,
    Pdng,
}

impl PaymentState {
    pub const ALL: [Self; 6] = [Self::Rcvd, Self::Actc, Self::Acsp, Self::Acsc, Self::Rjct, Self::Pdng];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Rcvd => "RCVD",
            Self::Actc => "ACTC",
            Self::Acsp => "ACSP",
            Self::Acsc => "ACSC",
            Self::Rjct => "RJCT",
            Self::Pdng => "PDNG",
        }
    }
}

impl FromStr for PaymentState {
    type Err = InvalidValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|state| state.code() == s)
            .ok_or(InvalidValue::new("PaymentState", "a listed payment state code"))
    }
}

impl fmt::Display for PaymentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_round_trips_and_nothing_else_parses() {
        for state in PaymentState::ALL {
            assert_eq!(state.code().parse(), Ok(state));
        }
        assert!("UNKN".parse::<PaymentState>().is_err());
        assert!("acsp".parse::<PaymentState>().is_err());
    }
}
