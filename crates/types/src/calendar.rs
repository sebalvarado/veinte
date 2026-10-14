use std::collections::BTreeSet;

use jiff::Timestamp;
use jiff::civil::{Date, DateTime, Time, Weekday};
use jiff::tz::Offset;

use crate::InvalidValue;

pub const SCHEME_OFFSET: Offset = Offset::constant(-5);

pub fn to_scheme_time(at: Timestamp) -> DateTime {
    SCHEME_OFFSET.to_datetime(at)
}

pub fn from_scheme_time(dt: DateTime) -> Timestamp {
    SCHEME_OFFSET.to_timestamp(dt).expect("a fixed offset maps every civil time in range")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusinessCalendar {
    opens: Time,
    closes: Time,
    settlements: Vec<Time>,
    reconciliation_close: Time,
    holidays: BTreeSet<Date>,
}

impl BusinessCalendar {
    pub fn new(
        opens: Time,
        closes: Time,
        settlements: impl IntoIterator<Item = Time>,
        reconciliation_close: Time,
        holidays: impl IntoIterator<Item = Date>,
    ) -> Result<Self, InvalidValue> {
        let settlements: Vec<Time> = settlements.into_iter().collect();
        if opens >= closes {
            return Err(InvalidValue::new("BusinessCalendar", "the window opens before it closes"));
        }
        if settlements.is_empty() || settlements.windows(2).any(|w| w[0] >= w[1]) {
            return Err(InvalidValue::new("BusinessCalendar", "settlement times are non-empty and increasing"));
        }
        Ok(Self { opens, closes, settlements, reconciliation_close, holidays: holidays.into_iter().collect() })
    }

    pub fn is_business_day(&self, date: Date) -> bool {
        !matches!(date.weekday(), Weekday::Saturday | Weekday::Sunday) && !self.holidays.contains(&date)
    }

    pub fn next_business_day(&self, date: Date) -> Date {
        let mut next = date;
        loop {
            next = next.tomorrow().expect("business days exist before year 9999");
            if self.is_business_day(next) {
                return next;
            }
        }
    }

    pub fn is_open(&self, at: Timestamp) -> bool {
        let dt = to_scheme_time(at);
        self.is_business_day(dt.date()) && (self.opens..self.closes).contains(&dt.time())
    }

    pub fn business_date(&self, at: Timestamp) -> Date {
        let dt = to_scheme_time(at);
        let date = dt.date();
        if self.is_business_day(date) && dt.time() < self.closes { date } else { self.next_business_day(date) }
    }

    pub fn reconciliation_close(&self, date: Date) -> Timestamp {
        from_scheme_time(date.to_datetime(self.reconciliation_close))
    }

    pub fn settlements(&self, date: Date) -> impl Iterator<Item = Timestamp> + '_ {
        let times = if self.is_business_day(date) { &self.settlements[..] } else { &[] };
        times.iter().map(move |t| from_scheme_time(date.to_datetime(*t)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::{date, time};

    fn calendar() -> BusinessCalendar {
        BusinessCalendar::new(
            time(8, 0, 0, 0),
            time(17, 0, 0, 0),
            [time(11, 0, 0, 0), time(15, 0, 0, 0), time(17, 30, 0, 0)],
            time(18, 0, 0, 0),
            [date(2026, 11, 2)],
        )
        .unwrap()
    }

    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    #[test]
    fn scheme_time_is_utc_minus_five() {
        assert_eq!(to_scheme_time(at("2026-10-12T14:00:00Z")), date(2026, 10, 12).at(9, 0, 0, 0));
        assert_eq!(from_scheme_time(date(2026, 10, 12).at(18, 0, 0, 0)), at("2026-10-12T23:00:00Z"));
    }

    #[test]
    fn weekends_and_holidays_are_not_business_days() {
        let cal = calendar();
        assert!(cal.is_business_day(date(2026, 10, 30)));
        assert!(!cal.is_business_day(date(2026, 10, 31)));
        assert!(!cal.is_business_day(date(2026, 11, 2)));
        assert_eq!(cal.next_business_day(date(2026, 10, 30)), date(2026, 11, 3));
    }

    #[test]
    fn a_payment_after_hours_belongs_to_the_next_business_day() {
        let cal = calendar();
        assert_eq!(cal.business_date(at("2026-10-30T21:59:59Z")), date(2026, 10, 30));
        assert_eq!(cal.business_date(at("2026-10-30T22:00:00Z")), date(2026, 11, 3));
        assert_eq!(cal.business_date(at("2026-10-31T15:00:00Z")), date(2026, 11, 3));
        assert_eq!(cal.business_date(at("2026-11-03T10:00:00Z")), date(2026, 11, 3));
        assert!(!cal.is_open(at("2026-11-03T10:00:00Z")));
        assert!(cal.is_open(at("2026-11-03T13:00:00Z")));
    }

    #[test]
    fn closes_and_settlements_are_scheme_times() {
        let cal = calendar();
        assert_eq!(cal.reconciliation_close(date(2026, 10, 30)), at("2026-10-30T23:00:00Z"));
        assert_eq!(cal.settlements(date(2026, 10, 30)).count(), 3);
        assert_eq!(cal.settlements(date(2026, 10, 31)).count(), 0);
    }

    #[test]
    fn refuses_inconsistent_data() {
        let t = |h| time(h, 0, 0, 0);
        assert!(BusinessCalendar::new(t(17), t(8), [t(11)], t(18), []).is_err());
        assert!(BusinessCalendar::new(t(8), t(17), [], t(18), []).is_err());
        assert!(BusinessCalendar::new(t(8), t(17), [t(15), t(11)], t(18), []).is_err());
    }
}
