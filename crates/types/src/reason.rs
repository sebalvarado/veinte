use std::fmt;
use std::str::FromStr;

use crate::InvalidValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Responsible {
    Beneficiary,
    Sip,
    Any,
}

macro_rules! reason_codes {
    ($($variant:ident $iso:literal $internal:expr, $party:ident;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum ReasonCode {
            $($variant,)*
        }

        impl ReasonCode {
            pub const ALL: [Self; 21] = [$(Self::$variant),*];

            pub const fn code(self) -> &'static str {
                match self { $(Self::$variant => $iso,)* }
            }

            pub const fn internal(self) -> Option<&'static str> {
                match self { $(Self::$variant => $internal,)* }
            }

            pub const fn responsible(self) -> Responsible {
                match self { $(Self::$variant => Responsible::$party,)* }
            }
        }
    };
}

reason_codes! {
    Ac04 "AC04" Some("0201"), Beneficiary;
    Ac01 "AC01" Some("0202"), Beneficiary;
    Ac06 "AC06" Some("0203"), Beneficiary;
    Ac05 "AC05" Some("0204"), Beneficiary;
    Dt01 "DT01" Some("0205"), Any;
    Ac07 "AC07" Some("0206"), Beneficiary;
    Am04 "AM04" Some("0207"), Sip;
    Ms01 "MS01" Some("0208"), Sip;
    Am11 "AM11" None, Sip;
    Ch16 "CH16" None, Sip;
    Ch21 "CH21" None, Sip;
    Dt02 "DT02" None, Sip;
    Ds0k "DS0K" None, Sip;
    Ch20 "CH20" None, Sip;
    Am01 "AM01" None, Sip;
    Du01 "DU01" None, Sip;
    Am02 "AM02" None, Sip;
    Ac02 "AC02" None, Sip;
    Ta01 "TA01" None, Sip;
    Ff10 "FF10" None, Sip;
    Ff02 "FF02" None, Sip;
}

impl FromStr for ReasonCode {
    type Err = InvalidValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL.into_iter().find(|r| r.code() == s).ok_or(InvalidValue::new("ReasonCode", "a listed reason code"))
    }
}

impl fmt::Display for ReasonCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReceivedReason {
    Listed(ReasonCode),
    Unlisted([u8; 4]),
}

impl ReceivedReason {
    pub fn code(&self) -> &str {
        match self {
            Self::Listed(r) => r.code(),
            Self::Unlisted(b) => std::str::from_utf8(b).expect("validated as ASCII"),
        }
    }
}

impl FromStr for ReceivedReason {
    type Err = InvalidValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(listed) = s.parse() {
            return Ok(Self::Listed(listed));
        }
        <[u8; 4]>::try_from(s.as_bytes())
            .ok()
            .filter(|b| b.iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
            .map(Self::Unlisted)
            .ok_or(InvalidValue::new("ReceivedReason", "four uppercase ASCII letters or digits"))
    }
}

impl fmt::Display for ReceivedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_tabla_12() {
        let beneficiary: Vec<_> = ReasonCode::ALL
            .into_iter()
            .filter(|r| r.responsible() == Responsible::Beneficiary)
            .map(ReasonCode::code)
            .collect();
        assert_eq!(beneficiary, ["AC04", "AC01", "AC06", "AC05", "AC07"]);
        let internal: Vec<_> = ReasonCode::ALL.into_iter().filter_map(ReasonCode::internal).collect();
        assert_eq!(internal, ["0201", "0202", "0203", "0204", "0205", "0206", "0207", "0208"]);
        assert_eq!(ReasonCode::Dt01.responsible(), Responsible::Any);
        assert_eq!(ReasonCode::Am04.responsible(), Responsible::Sip);
    }

    #[test]
    fn every_code_round_trips() {
        for r in ReasonCode::ALL {
            assert_eq!(r.code().parse(), Ok(r));
            assert_eq!(r.code().parse(), Ok(ReceivedReason::Listed(r)));
        }
    }

    #[test]
    fn keeps_unlisted_codes_as_evidence() {
        let received: ReceivedReason = "NARR".parse().unwrap();
        assert_eq!(received, ReceivedReason::Unlisted(*b"NARR"));
        assert_eq!(received.to_string(), "NARR");
        assert!("NARR".parse::<ReasonCode>().is_err());
        for s in ["narr", "NAR", "NARRA", "NA-R"] {
            assert!(s.parse::<ReceivedReason>().is_err(), "{s:?} parsed");
        }
    }
}
