//! Rules of the link between an integrante and its red.
//!
//! Independent project. Not affiliated with, nor endorsed by, the Banco Central del Ecuador.

use jiff::{SignedDuration, Timestamp};
use veinte_types::{InvalidValue, PaymentState, ReasonCode, ReceivedReason, Responsible};

use crate::{CreditTransfer, PACS_008, StatusReport};

const RESOLUTION_WINDOW: SignedDuration = SignedDuration::from_secs(60);
const END_TO_END_MAX: SignedDuration = SignedDuration::from_secs(20);

/// What the catálogo says once a stage's time has passed.
///
/// The 60 seconds of §5 regla 3 are not a stage of the payment. They are the customer's
/// window to confirm after the llave is resolved, before any order exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    /// §5 regla 4: the payment is rejected. The rule does not name a Tabla 12 code.
    Rejected,
    /// §11.3 excepción 1 and excepción 2: the ordering integrante tells the customer the
    /// payment is "en proceso".
    InProcess,
    /// §11.3 excepción 3: the SIP marks the payment "por conciliar". This is not a rejection.
    Reconciling,
    /// §11.3 excepción 4: the ordering integrante tells the customer "pago en proceso".
    PaymentInProcess,
    /// [assumption] §11.3 paso 7 does not say what a late credit is. The integrante sends nothing.
    Unanswered,
}

/// A stage of a payment that already exists.
///
/// Catálogo Operacional §5 regla 13: every stage has a time limit, and the values are parameters.
/// §11.3 is the payment flow and its four exceptions. §11.4 is only the summary table.
/// Credit is §11.3 paso 7, not an exception.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// §5 regla 4, from the start of the payment to its confirmation.
    EndToEnd,
    /// §11.3 excepción 1: the ordering red does not reach the SIP.
    ToSip,
    /// §11.3 excepción 2: the SIP does not reach the beneficiary red.
    Deliver,
    /// §11.3 excepción 3: the beneficiary red does not confirm to the SIP.
    Confirm,
    /// §11.3 excepción 4: a technical error of the SIP towards the ordering red.
    NotifyOrdering,
    /// §11.3 paso 7: the beneficiary integrante credits the customer.
    Credit,
}

impl Stage {
    pub const fn notice(self) -> Notice {
        match self {
            Self::EndToEnd => Notice::Rejected,
            Self::ToSip | Self::Deliver => Notice::InProcess,
            Self::Confirm => Notice::Reconciling,
            Self::NotifyOrdering => Notice::PaymentInProcess,
            Self::Credit => Notice::Unanswered,
        }
    }
}

/// Per-stage limits.
///
/// [assumption] The catálogo does not publish the hop values. The defaults are 3 s to reach
/// the SIP, 3 s to deliver to the beneficiary red, 10 s for that red to confirm, 3 s for the
/// SIP to answer the ordering red, and 8 s for the beneficiary integrante to credit. They nest
/// inside the 20 s ceiling of §5 regla 4: the three hops that precede the confirmation sum to
/// less than that ceiling, and the notice to the ordering red fits inside it.
///
/// [assumption] `credit` has to be shorter than `confirm`. The catálogo does not say so.
/// A stage is over only when its limit has been passed ("superado"), so the instant of the
/// limit itself is still inside the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StageLimits {
    end_to_end: SignedDuration,
    to_sip: SignedDuration,
    deliver: SignedDuration,
    confirm: SignedDuration,
    notify_ordering: SignedDuration,
    credit: SignedDuration,
}

impl StageLimits {
    pub fn new(
        end_to_end: SignedDuration,
        to_sip: SignedDuration,
        deliver: SignedDuration,
        confirm: SignedDuration,
        notify_ordering: SignedDuration,
        credit: SignedDuration,
    ) -> Result<Self, InvalidValue> {
        let limits = Self { end_to_end, to_sip, deliver, confirm, notify_ordering, credit };
        let hops = to_sip.checked_add(deliver).and_then(|sum| sum.checked_add(confirm));
        let nested = end_to_end.is_positive()
            && end_to_end <= END_TO_END_MAX
            && [to_sip, deliver, confirm, notify_ordering, credit].iter().all(|d| d.is_positive())
            && credit < confirm
            && notify_ordering <= end_to_end
            && hops.is_some_and(|sum| sum < end_to_end);
        if nested { Ok(limits) } else { Err(InvalidValue::new("StageLimits", "limits inside the catalogue ceiling")) }
    }

    pub fn of(self, stage: Stage) -> SignedDuration {
        match stage {
            Stage::EndToEnd => self.end_to_end,
            Stage::ToSip => self.to_sip,
            Stage::Deliver => self.deliver,
            Stage::Confirm => self.confirm,
            Stage::NotifyOrdering => self.notify_ordering,
            Stage::Credit => self.credit,
        }
    }
}

impl Default for StageLimits {
    fn default() -> Self {
        let s = SignedDuration::from_secs;
        Self::new(s(20), s(3), s(3), s(10), s(3), s(8)).expect("the published defaults fit the ceiling")
    }
}

/// Catálogo Operacional §5 regla 3. `true` once 60 seconds have passed since the llave was
/// resolved. The customer confirms inside that window, before any order exists. Passing it
/// is not a rejection of a payment.
pub fn resolution_lapsed(resolved: Timestamp, now: Timestamp) -> bool {
    overdue(RESOLUTION_WINDOW, resolved, now)
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
pub fn integrante_may_send(code: ReasonCode) -> bool {
    matches!(code.responsible(), Responsible::Beneficiary | Responsible::Any)
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
///
/// [assumption] When `OrgnlTxRef/DbtrAgt` is present it is the ordering integrante of the
/// order. Tabla 18 does not list the element.
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
    if report.original_debtor_agent.is_some_and(|agent| agent != order.debtor_agent) {
        return Err(refused(ReasonCode::Ch16, "the confirmation cites a different ordering integrante"));
    }
    Ok(())
}

/// Whether an integrante may put this confirmation on the link.
///
/// [assumption] A rejection carries a Tabla 12 code the integrante is responsible for.
/// An unlisted code, or a code assigned to the SIP, is `CH16`. A confirmation that is not a
/// rejection carries no code. `AM02` is not judged here: §5 regla 8 leaves the maximum amount
/// as a parameter and does not publish it.
pub fn integrante_report(report: &StatusReport) -> Result<(), ProfileError> {
    match (report.status, report.reason) {
        (PaymentState::Rjct, Some(ReceivedReason::Listed(code))) if integrante_may_send(code) => Ok(()),
        (PaymentState::Rjct, Some(reason)) => {
            Err(refused(ReasonCode::Ch16, format!("the integrante does not send {reason}")))
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
        report.original_debtor_agent = Some(order.debtor_agent);
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
        wrong.original_msg_id = "OTRO-MENSAJE".into();
        wrong.original_instr_id = "BALFECEQ12102026090000000002".into();
        wrong.original_end_to_end_id = "OTRO-EXTREMO".into();
        assert_eq!(correlate(order, &wrong).unwrap_err().reason, ReasonCode::Ch16);
    }

    #[test]
    fn the_integrante_sends_only_its_tabla_12_codes() {
        let (order_msg, _) = sample::order();
        let Document::CreditTransfer(order) = &order_msg.document else { unreachable!() };
        for code in ReasonCode::ALL {
            let report = report_for(order, Some(ReceivedReason::Listed(code)));
            assert_eq!(integrante_report(&report).is_ok(), integrante_may_send(code), "{code}");
        }
        let accepted = report_for(order, None);
        assert_eq!(integrante_report(&accepted), Ok(()));
        let unlisted = report_for(order, Some("NARR".parse().unwrap()));
        assert_eq!(integrante_report(&unlisted).unwrap_err().reason, ReasonCode::Ch16);
        assert!(integrante_may_send(ReasonCode::Dt01));
        assert!(!integrante_may_send(ReasonCode::Am04));
    }

    #[test]
    fn each_stage_keeps_the_catalogue_outcome() {
        assert_eq!(Stage::EndToEnd.notice(), Notice::Rejected);
        assert_eq!(Stage::ToSip.notice(), Notice::InProcess);
        assert_eq!(Stage::Deliver.notice(), Notice::InProcess);
        assert_eq!(Stage::Confirm.notice(), Notice::Reconciling);
        assert_eq!(Stage::NotifyOrdering.notice(), Notice::PaymentInProcess);
        assert_eq!(Stage::Credit.notice(), Notice::Unanswered);
    }

    #[test]
    fn a_stage_is_over_only_after_its_limit() {
        let limits = StageLimits::default();
        let started: Timestamp = "2026-10-12T14:00:00Z".parse().unwrap();
        let stages =
            [Stage::EndToEnd, Stage::ToSip, Stage::Deliver, Stage::Confirm, Stage::NotifyOrdering, Stage::Credit];
        for stage in stages {
            let limit = limits.of(stage);
            let at_limit = started.checked_add(limit).unwrap();
            assert!(!overdue(limit, started, at_limit), "{stage:?} expired on its limit");
            let past = at_limit.checked_add(SignedDuration::from_micros(1)).unwrap();
            assert!(overdue(limit, started, past), "{stage:?} stayed open");
        }
        let window = started.checked_add(SignedDuration::from_secs(60)).unwrap();
        assert!(!resolution_lapsed(started, window));
        let lapsed = window.checked_add(SignedDuration::from_micros(1)).unwrap();
        assert!(resolution_lapsed(started, lapsed));
    }

    #[test]
    fn a_limit_outside_the_ceiling_is_refused() {
        let limits = StageLimits::default();
        assert_eq!(limits.of(Stage::EndToEnd), SignedDuration::from_secs(20));
        assert_eq!(limits.of(Stage::Credit), SignedDuration::from_secs(8));
        assert!(
            StageLimits::new(
                SignedDuration::from_secs(21),
                limits.of(Stage::ToSip),
                limits.of(Stage::Deliver),
                limits.of(Stage::Confirm),
                limits.of(Stage::NotifyOrdering),
                limits.of(Stage::Credit)
            )
            .is_err()
        );
        assert!(
            StageLimits::new(
                limits.of(Stage::EndToEnd),
                limits.of(Stage::ToSip),
                limits.of(Stage::Deliver),
                limits.of(Stage::Confirm),
                limits.of(Stage::NotifyOrdering),
                limits.of(Stage::Confirm)
            )
            .is_err()
        );
    }
}
