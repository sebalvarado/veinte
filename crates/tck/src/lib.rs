//! Scenarios for the link an integrante speaks with its red.
//!
//! Independent project. Not affiliated with, nor endorsed by, the Banco Central del Ecuador.

use jiff::{SignedDuration, Timestamp};
use veinte_iso20022::{
    AppHdr, BusinessMessage, Channel, CreditTransfer, Document, Initiation, Keyring, OpenError, PACS_008, Party,
    Signer, Stage, StageLimits, StatusReport, connector_may_send, connector_report, correlate, decode, open_message,
};
use veinte_types::{Bic, PaymentState, ReasonCode, ReceivedReason};

/// What the reference connector decided to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Credit,
    Reject(ReasonCode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Profile(veinte_iso20022::ProfileError),
    Timeout(Stage),
    Signature(OpenError),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Profile(err) => write!(f, "{err}"),
            Self::Timeout(stage) => write!(f, "timeout {stage:?}"),
            Self::Signature(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for Refusal {}

/// The connector the scenarios drive. It speaks only the open profile.
#[derive(Debug, Clone)]
pub struct Reference {
    limits: StageLimits,
}

impl Reference {
    pub fn new(limits: StageLimits) -> Self {
        Self { limits }
    }

    pub fn limits(&self) -> StageLimits {
        self.limits
    }

    /// Catálogo Operacional §5 regla 13. Past the limit, the stage is over.
    ///
    /// [assumption] Tabla 12 names `DT01` for a timeout and does not publish a different code
    /// for each stage, so every stage uses `DT01`. The instant of the limit is still inside
    /// the window: the catálogo says the time has to be exceeded ("superado").
    pub fn within(&self, stage: Stage, started: Timestamp, now: Timestamp) -> Result<(), Refusal> {
        if veinte_iso20022::overdue(self.limits.of(stage), started, now) {
            Err(Refusal::Timeout(stage))
        } else {
            Ok(())
        }
    }

    pub fn answer(
        &self,
        from: Bic,
        to: Bic,
        order: &CreditTransfer,
        decision: Decision,
        received: Timestamp,
        now: Timestamp,
    ) -> Result<BusinessMessage, Refusal> {
        if self.within(Stage::Credit, received, now).is_err() {
            return Ok(self.report(from, to, order, PaymentState::Rjct, Some(ReasonCode::Dt01), now));
        }
        let message = match decision {
            Decision::Credit => self.report(from, to, order, PaymentState::Acsp, None, now),
            Decision::Reject(code) => self.report(from, to, order, PaymentState::Rjct, Some(code), now),
        };
        let Document::StatusReport(report) = &message.document else { unreachable!() };
        connector_report(report).map_err(Refusal::Profile)?;
        Ok(message)
    }

    pub fn take_report(
        &self,
        order: &CreditTransfer,
        report: &StatusReport,
        sent: Timestamp,
        now: Timestamp,
    ) -> Result<(), Refusal> {
        self.within(Stage::EndToEnd, sent, now)?;
        correlate(order, report).map_err(Refusal::Profile)
    }

    fn report(
        &self,
        from: Bic,
        to: Bic,
        order: &CreditTransfer,
        status: PaymentState,
        reason: Option<ReasonCode>,
        now: Timestamp,
    ) -> BusinessMessage {
        BusinessMessage {
            header: AppHdr {
                from,
                to,
                id: format!("C{}", now.as_microsecond()),
                created: now,
                possible_duplicate: false,
            },
            document: Document::StatusReport(StatusReport {
                msg_id: format!("C{}", now.as_microsecond()),
                created: now,
                beneficiary_agent: order.creditor_agent,
                original_msg_id: order.msg_id.clone(),
                original_msg_name: PACS_008.into(),
                original_instr_id: order.instr_id.clone(),
                original_end_to_end_id: order.end_to_end_id.clone(),
                status,
                detail: reason.map(|code| code.description().to_owned()),
                reason: reason.map(ReceivedReason::Listed),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub name: String,
    pub outcome: String,
}

pub struct Parties<'a> {
    pub ordering: &'a Signer,
    pub ordering_red: &'a Signer,
    pub beneficiary: &'a Signer,
    pub beneficiary_red: &'a Signer,
    pub now: Timestamp,
}

/// Runs the profile against the reference connector and returns one line per scenario.
pub fn examine(parties: Parties<'_>) -> Vec<Finding> {
    let now = parties.now;
    let reference = Reference::new(StageLimits::default());
    let mut findings = Vec::new();
    let order_msg = sample_order(now);
    let Document::CreditTransfer(order) = order_msg.document.clone() else { unreachable!() };

    let mut red_trust = Keyring::default();
    let trust = red_trust.trust(parties.ordering.bic(), parties.ordering.verifying_key());
    let sealed = parties.ordering.seal(parties.ordering_red.bic(), &order_msg);
    let opened = trust.ok().and_then(|_| open_message(&red_trust, &sealed).ok());

    let mut inbound = order_msg.clone();
    inbound.header.from = parties.beneficiary_red.bic();
    inbound.header.to = parties.beneficiary.bic();
    let mut beneficiary_trust = Keyring::default();
    let _ = beneficiary_trust.trust(parties.beneficiary_red.bic(), parties.beneficiary_red.verifying_key());
    let delivered = parties.beneficiary_red.seal(parties.beneficiary.bic(), &inbound);
    let inbound_ok = open_message(&beneficiary_trust, &delivered).ok().as_ref() == Some(&inbound);
    let answer =
        reference.answer(parties.beneficiary.bic(), parties.beneficiary_red.bic(), &order, Decision::Credit, now, now);
    let reply_ok = answer.as_ref().ok().and_then(|message| {
        let sealed = parties.beneficiary.seal(parties.beneficiary_red.bic(), message);
        let mut red = Keyring::default();
        red.trust(parties.beneficiary.bic(), parties.beneficiary.verifying_key()).ok()?;
        let opened = open_message(&red, &sealed).ok()?;
        let Document::StatusReport(report) = &opened.document else { return None };
        reference.take_report(&order, report, now, now).ok()?;
        Some(opened == *message && decode(&sealed.body).ok().as_ref() == Some(message))
    });
    findings.push(line(
        "orden pacs.008",
        if opened.as_ref() == Some(&order_msg) && inbound_ok { "aceptada" } else { "falló" },
    ));
    findings.push(line("confirmación pacs.002", if reply_ok == Some(true) { "aceptada" } else { "falló" }));

    let mut tampered = parties.ordering.seal(parties.ordering_red.bic(), &order_msg);
    tampered.body[40] ^= 0x01;
    let bad_signature = matches!(open_message(&red_trust, &tampered), Err(OpenError::Rejected(_)));
    findings.push(line("firma alterada", if bad_signature { "rechazada" } else { "falló" }));

    let drifted = answer.ok().and_then(|message| match message.document {
        Document::StatusReport(mut report) => {
            report.original_instr_id = "OTRA".into();
            correlate(&order, &report).err()
        }
        _ => None,
    });
    findings.push(line("correlación distinta", if drifted.is_some() { "rechazada" } else { "falló" }));

    for (name, stage) in [
        ("timeout resolución de la llave", Stage::Resolution),
        ("timeout extremo a extremo", Stage::EndToEnd),
        ("timeout envío al SIP", Stage::ToSip),
        ("timeout entrega a la red beneficiaria", Stage::Deliver),
        ("timeout confirmación de la red beneficiaria", Stage::Confirm),
        ("timeout acreditación", Stage::Credit),
    ] {
        let limit = reference.limits().of(stage);
        let at_limit = now.checked_add(limit).unwrap();
        let past = at_limit.checked_add(SignedDuration::from_micros(1)).unwrap();
        let recognized = reference.within(stage, now, at_limit).is_ok()
            && matches!(reference.within(stage, now, past), Err(Refusal::Timeout(_)));
        let code_ok = stage != Stage::Credit
            || credit_timeout(&reference, parties.beneficiary.bic(), parties.beneficiary_red.bic(), &order, now, past);
        let outcome = if recognized && code_ok { ReasonCode::Dt01.code() } else { "falló" };
        findings.push(line(name, outcome));
    }

    for code in ReasonCode::ALL {
        let result = reference.answer(
            parties.beneficiary.bic(),
            parties.beneficiary_red.bic(),
            &order,
            Decision::Reject(code),
            now,
            now,
        );
        let allowed = connector_may_send(code);
        let matches_rule = match &result {
            Ok(message) => {
                allowed
                    && matches!(&message.document, Document::StatusReport(report)
                        if report.reason == Some(ReceivedReason::Listed(code)) && correlate(&order, report).is_ok())
            }
            Err(Refusal::Profile(_)) => !allowed,
            Err(_) => false,
        };
        let outcome = if !matches_rule {
            "falló"
        } else if allowed {
            "permitido"
        } else {
            "no permitido"
        };
        findings.push(line(format!("código {code}"), outcome));
    }

    findings
}

fn credit_timeout(
    reference: &Reference,
    from: Bic,
    to: Bic,
    order: &CreditTransfer,
    received: Timestamp,
    now: Timestamp,
) -> bool {
    let Ok(message) = reference.answer(from, to, order, Decision::Credit, received, now) else { return false };
    let Document::StatusReport(report) = &message.document else { return false };
    report.status == PaymentState::Rjct
        && report.reason == Some(ReceivedReason::Listed(ReasonCode::Dt01))
        && correlate(order, report).is_ok()
        && connector_may_send(ReasonCode::Dt01)
}

fn line(name: impl Into<String>, outcome: impl Into<String>) -> Finding {
    Finding { name: name.into(), outcome: outcome.into() }
}

/// Spanish report a red can read. Every line is one scenario.
pub fn render(findings: &[Finding]) -> String {
    let mut out = String::from(
        "Informe de conformidad del enlace\n\
         \n\
         Independiente. No está afiliado ni respaldado por el Banco Central del Ecuador.\n\
         \n\
         Perfil del enlace entre un integrante y su red de pagos. Las reglas citan el Catálogo Operacional. \
         Los tiempos de 60 s y 20 s son los del catálogo. Los demás tiempos de etapa son parámetros: el catálogo no publica los valores.\n\
         \n",
    );
    for finding in findings {
        out.push_str(&finding.name);
        out.push_str(": ");
        out.push_str(&finding.outcome);
        out.push('\n');
    }
    out
}

pub fn passed(findings: &[Finding]) -> bool {
    findings.iter().all(|finding| finding.outcome != "falló")
}

fn sample_order(now: Timestamp) -> BusinessMessage {
    let bic = |s: &str| -> Bic { s.parse().unwrap() };
    let party = |name: &str, id: &str, account: &str| Party {
        name: name.into(),
        id: id.into(),
        id_scheme: "NIDN".into(),
        account: account.into(),
    };
    BusinessMessage {
        header: AppHdr {
            from: bic("BALFECEQ"),
            to: bic("RALFECEQ"),
            id: "M0000000000000000000000000000001".into(),
            created: now,
            possible_duplicate: false,
        },
        document: Document::CreditTransfer(Box::new(CreditTransfer {
            msg_id: "M0000000000000000000000000000001".into(),
            created: now,
            instr_id: "BALFECEQ12102026090000000001".into(),
            end_to_end_id: "BALFECEQ12102026090000000001".into(),
            amount: "25.40".parse().unwrap(),
            instructing_agent: bic("RALFECEQ"),
            instructed_agent: bic("RBETECEQ"),
            debtor: party("María Ejemplo", "0000000001", "1100000001"),
            debtor_agent: bic("BALFECEQ"),
            creditor: party("Julio Tienda", "0000000002", "4400000001"),
            creditor_agent: bic("CBETECEQ"),
            channel: Channel::Web,
            initiation: Initiation::Credential,
            purpose: "abono".into(),
            qr: None,
            marked_at: None,
        })),
    }
}
