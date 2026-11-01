//! Rules of the link between an integrante and its red.
//!
//! Independent project. Not affiliated with, nor endorsed by, the Banco Central del Ecuador.

use jiff::{SignedDuration, Timestamp};
use veinte_types::{InvalidValue, PaymentState, ReasonCode, ReceivedReason, Responsible};

use crate::{CreditTransfer, PACS_008, StatusReport};

/// Catálogo Operacional §5 regla 3: once the llave is resolved, the customer has 60 seconds.
pub const RESOLUTION_CEILING: SignedDuration = SignedDuration::from_secs(60);

/// Catálogo Operacional §5 regla 4: the payment itself cannot take more than 20 seconds.
pub const END_TO_END_CEILING: SignedDuration = SignedDuration::from_secs(20);

/// A stage of the payment, each with its own timeout.
///
/// Catálogo Operacional §5 regla 13: every stage has a timeout, and the values are parameters.
/// The resolution window and the end-to-end ceiling are the two durations the catálogo states.
/// The other four match the hops of §11.3 and the four exceptions of §11.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// §5 regla 3, before the order exists.
    Resolution,
    /// §5 regla 4, from the start of the payment to its confirmation.
    EndToEnd,
    /// §11.4 excepción 1: the ordering red does not reach the SIP.
    ToSip,
    /// §11.4 excepción 2: the SIP does not reach the beneficiary red.
    Deliver,
    /// §11.4 excepción 3: the beneficiary red does not confirm to the SIP.
    Confirm,
    /// §11.3 paso 7: the beneficiary integrante credits the customer.
    Credit,
}

impl Stage {
    pub const ALL: [Self; 6] =
        [Self::Resolution, Self::EndToEnd, Self::ToSip, Self::Deliver, Self::Confirm, Self::Credit];
}

/// Per-stage timeouts.
///
/// [assumption] The catálogo does not publish the four hop values. The defaults below are this
/// profile's: 3 s to reach the SIP, 3 s to deliver to the beneficiary red, 10 s for that red to
/// confirm, 8 s for the beneficiary integrante to credit. They nest inside the 20 s ceiling:
/// the credit is shorter than the confirmation, and the three hops that precede the end of the
/// confirmation sum to less than the end-to-end limit. A stage is over only when its limit has
/// been passed ("superado"), so the instant of the limit itself is still inside the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StageLimits {
    resolution: SignedDuration,
    end_to_end: SignedDuration,
    to_sip: SignedDuration,
    deliver: SignedDuration,
    confirm: SignedDuration,
    credit: SignedDuration,
}

impl StageLimits {
    pub fn new(
        resolution: SignedDuration,
        end_to_end: SignedDuration,
        to_sip: SignedDuration,
        deliver: SignedDuration,
        confirm: SignedDuration,
        credit: SignedDuration,
    ) -> Result<Self, InvalidValue> {
        let limits = Self { resolution, end_to_end, to_sip, deliver, confirm, credit };
        let hops = to_sip.checked_add(deliver).and_then(|sum| sum.checked_add(confirm));
        let nested = resolution.is_positive()
            && resolution <= RESOLUTION_CEILING
            && end_to_end.is_positive()
            && end_to_end <= END_TO_END_CEILING
            && [to_sip, deliver, confirm, credit].iter().all(|d| d.is_positive())
            && credit < confirm
            && hops.is_some_and(|sum| sum < end_to_end);
        if nested {
            Ok(limits)
        } else {
            Err(InvalidValue::new("StageLimits", "timeouts inside the catálogo ceilings"))
        }
    }

    pub fn of(self, stage: Stage) -> SignedDuration {
        match stage {
            Stage::Resolution => self.resolution,
            Stage::EndToEnd => self.end_to_end,
            Stage::ToSip => self.to_sip,
            Stage::Deliver => self.deliver,
            Stage::Confirm => self.confirm,
            Stage::Credit => self.credit,
        }
    }
}

impl Default for StageLimits {
    fn default() -> Self {
        let s = SignedDuration::from_secs;
        Self::new(s(60), s(20), s(3), s(3), s(10), s(8)).expect("the published defaults fit the ceilings")
    }
}

/// `true` once `limit` has been passed. The instant of the limit is still on time.
pub fn overdue(limit: SignedDuration, started: Timestamp, now: Timestamp) -> bool {
    match started.checked_add(limit) {
        Ok(deadline) => now > deadline,
        Err(_) => true,
    }
}

/// Catálogo Operacional §5 numeral 7, Tabla 12. The integrante beneficiario sends the codes
/// in its column, and `DT01`, which the table assigns to every party.
pub fn connector_may_send(code: ReasonCode) -> bool {
    matches!(code.responsible(), Responsible::Beneficiary | Responsible::Any)
}

/// Catálogo Operacional §5 numeral 7, Tabla 12, column RESPONSABLE `SIP`, plus `DT01`.
pub fn sip_may_send(code: ReasonCode) -> bool {
    matches!(code.responsible(), Responsible::Sip | Responsible::Any)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileError {
    pub reason: ReasonCode,
    pub detail: String,
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.reason, self.detail)
    }
}

impl std::error::Error for ProfileError {}

fn refused(reason: ReasonCode, detail: impl Into<String>) -> ProfileError {
    ProfileError { reason, detail: detail.into() }
}

/// The ids that tie a `pacs.002` to its `pacs.008`.
///
/// Catálogo Operacional §8, Tabla 18: `OrgnlMsgId`, `OrgnlMsgNmId`, `OrgnlInstrId`,
/// `OrgnlEndToEndId`.
///
/// [assumption] `OrgnlMsgId` cites `GrpHdr/MsgId` of the order. The catálogo calls both that
/// element and the application header "the unique message identifier" and does not say which
/// one the confirmation repeats. A mismatch is `CH16`; Tabla 12 does not name a code for it.
pub fn correlate(order: &CreditTransfer, report: &StatusReport) -> Result<(), ProfileError> {
    if report.original_msg_id != order.msg_id {
        return Err(refused(ReasonCode::Ch16, "the confirmation cites a different message"));
    }
    if report.original_msg_name != PACS_008 {
        return Err(refused(ReasonCode::Ch16, "the confirmation does not cite a payment order"));
    }
    if report.original_instr_id != order.instr_id {
        return Err(refused(ReasonCode::Ch16, "the confirmation cites a different payment"));
    }
    if report.original_end_to_end_id != order.end_to_end_id {
        return Err(refused(ReasonCode::Ch16, "the confirmation cites a different end-to-end id"));
    }
    Ok(())
}

/// Whether a connector may put this confirmation on the link.
///
/// [assumption] A rejection carries a Tabla 12 code the integrante is responsible for.
/// An unlisted code, or a code assigned to the SIP, is `CH16`. A confirmation that is not a
/// rejection carries no code. `AM02` is not judged here: §5 regla 8 leaves the maximum amount
/// as a parameter and does not publish it.
pub fn connector_report(report: &StatusReport) -> Result<(), ProfileError> {
    match (report.status, report.reason) {
        (PaymentState::Rjct, Some(ReceivedReason::Listed(code))) if connector_may_send(code) => Ok(()),
        (PaymentState::Rjct, Some(reason)) => {
            Err(refused(ReasonCode::Ch16, format!("the connector does not send {reason}")))
        }
        (PaymentState::Rjct, None) => Err(refused(ReasonCode::Ch21, "a rejection needs a reason")),
        (_, None) => Ok(()),
        (_, Some(reason)) => Err(refused(ReasonCode::Ch16, format!("a confirmation carries no reason, got {reason}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::sample;
    use crate::{Document, PaymentState, ReasonCode, ReceivedReason};
    use jiff::SignedDuration;

    fn report_for(order: &CreditTransfer, reason: Option<ReceivedReason>) -> StatusReport {
        let (_, Document::StatusReport(report)) = sample::status(reason) else { unreachable!() };
        let mut report = report;
        report.original_msg_id = order.msg_id.clone();
        report.original_instr_id = order.instr_id.clone();
        report.original_end_to_end_id = order.end_to_end_id.clone();
        report.reason = reason;
        report.status = if reason.is_some() { PaymentState::Rjct } else { PaymentState::Acsp };
        report.detail = reason.map(|r| r.to_string());
        report
    }

    #[test]
    fn a_confirmation_cites_its_order() {
        let (order_msg, _) = sample::order();
        let Document::CreditTransfer(order) = &order_msg.document else { unreachable!() };
        let report = report_for(order, Some(ReceivedReason::Listed(ReasonCode::Ac04)));
        assert_eq!(correlate(order, &report), Ok(()));
        let mut wrong = report.clone();
        wrong.original_instr_id = "BALFECEQ12102026090000000002".into();
        assert_eq!(correlate(order, &wrong).unwrap_err().reason, ReasonCode::Ch16);
    }

    #[test]
    fn the_connector_sends_only_its_tabla_12_codes() {
        let (order_msg, _) = sample::order();
        let Document::CreditTransfer(order) = &order_msg.document else { unreachable!() };
        for code in ReasonCode::ALL {
            let report = report_for(order, Some(ReceivedReason::Listed(code)));
            assert_eq!(connector_report(&report).is_ok(), connector_may_send(code), "{code}");
        }
        let accepted = report_for(order, None);
        assert_eq!(connector_report(&accepted), Ok(()));
        let unlisted = report_for(order, Some("NARR".parse().unwrap()));
        assert_eq!(connector_report(&unlisted).unwrap_err().reason, ReasonCode::Ch16);
        assert!(connector_may_send(ReasonCode::Dt01));
        assert!(!connector_may_send(ReasonCode::Am04));
        assert!(sip_may_send(ReasonCode::Am04));
        assert!(sip_may_send(ReasonCode::Dt01));
        assert!(!sip_may_send(ReasonCode::Ac04));
    }

    #[test]
    fn a_stage_is_over_only_after_its_limit() {
        let limits = StageLimits::default();
        let started: Timestamp = "2026-10-12T14:00:00Z".parse().unwrap();
        for stage in Stage::ALL {
            let limit = limits.of(stage);
            let at_limit = started.checked_add(limit).unwrap();
            assert!(!overdue(limit, started, at_limit), "{stage:?} expired on its limit");
            let past = at_limit.checked_add(SignedDuration::from_micros(1)).unwrap();
            assert!(overdue(limit, started, past), "{stage:?} stayed open");
        }
    }

    #[test]
    fn defaults_match_the_ceilings_and_a_longer_window_is_refused() {
        let limits = StageLimits::default();
        assert_eq!(limits.of(Stage::Resolution), RESOLUTION_CEILING);
        assert_eq!(limits.of(Stage::EndToEnd), END_TO_END_CEILING);
        assert_eq!(limits.of(Stage::Credit), SignedDuration::from_secs(8));
        let too_long = SignedDuration::from_secs(21);
        assert!(
            StageLimits::new(
                RESOLUTION_CEILING,
                too_long,
                limits.of(Stage::ToSip),
                limits.of(Stage::Deliver),
                limits.of(Stage::Confirm),
                limits.of(Stage::Credit)
            )
            .is_err()
        );
    }
}
