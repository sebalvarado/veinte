//! Scenarios for the link an integrante speaks with its red.
//!
//! Independent project. Not affiliated with, nor endorsed by, the Banco Central del Ecuador.

use jiff::{SignedDuration, Timestamp};
use veinte_iso20022::{
    AppHdr, BusinessMessage, CreditTransfer, Document, Envelope, Keyring, Notice, OpenError, PACS_008, ProfileError,
    Stage, StageLimits, StatusReport, correlate, example_order, integrante_report, open_message, overdue,
    resolution_lapsed,
};
use veinte_types::{Bic, PaymentState, ReasonCode, ReceivedReason};

/// What the integrante was asked to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Credit,
    Reject(ReasonCode),
}

/// The integrante's answer to an order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Confirmation(Envelope),
    /// [assumption] §11.3 paso 7 does not say what a late credit is, so the integrante stays silent.
    Silent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    Signature(OpenError),
    Profile(ProfileError),
    Correlation(ProfileError),
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Signature(err) => write!(f, "{err}"),
            Self::Profile(err) => write!(f, "{err}"),
            Self::Correlation(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for Fault {}

/// The integrante side of the link. The kit calls this. It does not call the profile for the party.
pub trait Integrante {
    fn on_order(&mut self, envelope: &Envelope, decision: Decision, now: Timestamp) -> Result<Reply, Fault>;
    fn on_report(&mut self, envelope: &Envelope, now: Timestamp) -> Result<(), Fault>;
    fn on_stage(&self, stage: Stage, started: Timestamp, now: Timestamp) -> Option<Notice>;
    fn resolution_open(&self, resolved: Timestamp, now: Timestamp) -> bool;
}

/// The reference integrante. One implementor of [`Integrante`].
#[derive(Debug, Clone)]
pub struct Reference {
    signer: veinte_iso20022::Signer,
    peers: Keyring,
    red: Bic,
    limits: StageLimits,
    order: Option<CreditTransfer>,
}

impl Reference {
    pub fn new(
        signer: veinte_iso20022::Signer,
        red: Bic,
        red_key: [u8; 32],
        limits: StageLimits,
    ) -> Result<Self, veinte_iso20022::TrustError> {
        let mut peers = Keyring::default();
        peers.trust(red, red_key)?;
        Ok(Self { signer, peers, red, limits, order: None })
    }

    pub fn bic(&self) -> Bic {
        self.signer.bic()
    }

    pub fn verifying_key(&self) -> [u8; 32] {
        self.signer.verifying_key()
    }

    fn confirmation(
        &self,
        order: &CreditTransfer,
        status: PaymentState,
        reason: Option<ReasonCode>,
        now: Timestamp,
    ) -> BusinessMessage {
        let id = message_id(now);
        BusinessMessage {
            header: AppHdr {
                from: self.signer.bic(),
                to: self.red,
                id: id.clone(),
                created: now,
                possible_duplicate: false,
            },
            document: Document::StatusReport(StatusReport {
                msg_id: id,
                created: now,
                beneficiary_agent: order.creditor_agent,
                original_msg_id: order.msg_id.clone(),
                original_msg_name: PACS_008.into(),
                original_instr_id: order.instr_id.clone(),
                original_end_to_end_id: order.end_to_end_id.clone(),
                original_debtor_agent: Some(order.debtor_agent),
                status,
                reason: reason.map(ReceivedReason::Listed),
                // [assumption] AddtlInf carries the Tabla 12 description. Tabla 18 requires the
                // element on RJCT and does not say what it contains.
                detail: reason.map(|code| code.description().to_owned()),
            }),
        }
    }
}

impl Integrante for Reference {
    fn on_order(&mut self, envelope: &Envelope, decision: Decision, now: Timestamp) -> Result<Reply, Fault> {
        let message = open_message(&self.peers, envelope).map_err(Fault::Signature)?;
        let Document::CreditTransfer(order) = message.document else {
            return Err(Fault::Profile(profile("the link carries a payment order")));
        };
        let received = message.header.created;
        self.order = Some((*order).clone());
        if overdue(self.limits.of(Stage::Credit), received, now) {
            return Ok(Reply::Silent);
        }
        let (status, reason) = match decision {
            Decision::Credit => (PaymentState::Acsp, None),
            Decision::Reject(code) => (PaymentState::Rjct, Some(code)),
        };
        let reply = self.confirmation(&order, status, reason, now);
        let Document::StatusReport(report) = &reply.document else { unreachable!() };
        integrante_report(report).map_err(Fault::Profile)?;
        Ok(Reply::Confirmation(self.signer.seal(self.red, &reply)))
    }

    fn on_report(&mut self, envelope: &Envelope, _now: Timestamp) -> Result<(), Fault> {
        let message = open_message(&self.peers, envelope).map_err(Fault::Signature)?;
        let Document::StatusReport(report) = &message.document else {
            return Err(Fault::Profile(profile("the link carries a confirmation")));
        };
        let Some(order) = &self.order else {
            return Err(Fault::Profile(profile("the confirmation arrives before the order")));
        };
        correlate(order, report).map_err(Fault::Correlation)
    }

    fn on_stage(&self, stage: Stage, started: Timestamp, now: Timestamp) -> Option<Notice> {
        if overdue(self.limits.of(stage), started, now) { Some(stage.notice()) } else { None }
    }

    fn resolution_open(&self, resolved: Timestamp, now: Timestamp) -> bool {
        !resolution_lapsed(resolved, now)
    }
}

fn profile(detail: &str) -> ProfileError {
    ProfileError { reason: ReasonCode::Ch16, detail: detail.to_owned() }
}

fn message_id(now: Timestamp) -> String {
    format!("C{}", now.as_microsecond())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Accepted,
    Rejected,
    PaymentRejected,
    Allowed,
    NotAllowed,
    Lapsed,
    InProcess,
    Reconciling,
    PaymentInProcess,
    Unanswered,
    Failed,
}

impl Outcome {
    fn spanish(self) -> &'static str {
        match self {
            Self::Accepted => "aceptada",
            Self::Rejected => "rechazada",
            Self::PaymentRejected => "pago rechazado",
            Self::Allowed => "permitido",
            Self::NotAllowed => "no permitido",
            Self::Lapsed => "vencida",
            Self::InProcess => "en proceso",
            Self::Reconciling => "por conciliar",
            Self::PaymentInProcess => "pago en proceso",
            Self::Unanswered => "sin confirmación",
            Self::Failed => "no conforme",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub name: String,
    pub outcome: Outcome,
}

pub struct Link<'a> {
    pub red: &'a veinte_iso20022::Signer,
    pub integrante: Bic,
    pub integrante_key: [u8; 32],
    pub now: Timestamp,
}

/// Drives `party` through the signature, correlation, resolution and stage scenarios.
pub fn drive(party: &mut impl Integrante, link: &Link<'_>) -> Vec<Finding> {
    let now = link.now;
    let order_msg = example_order(link.red.bic(), link.integrante, now);
    let Document::CreditTransfer(order) = order_msg.document.clone() else { unreachable!() };
    let sealed = link.red.seal(link.integrante, &order_msg);
    let inbound = Envelope::parse(&sealed.to_bytes()).expect("a sealed envelope frames");

    let mut findings = Vec::new();
    match party.on_order(&inbound, Decision::Credit, now) {
        Ok(Reply::Confirmation(reply)) => {
            findings.push(line("orden pacs.008", Outcome::Accepted));
            let correlated = cites_order(link, &reply, &order);
            findings.push(line("confirmación pacs.002", if correlated { Outcome::Accepted } else { Outcome::Failed }));
        }
        _ => {
            findings.push(line("orden pacs.008", Outcome::Failed));
            findings.push(line("confirmación pacs.002", Outcome::Failed));
        }
    }

    let mut body = inbound.body.clone();
    body[0] ^= 0xff;
    let mut signature = *inbound.signature();
    signature[0] ^= 0xff;
    let tampered = Envelope::from_parts(inbound.from, inbound.to, body, signature);
    let refused = matches!(party.on_order(&tampered, Decision::Credit, now), Err(Fault::Signature(_)));
    findings.push(line("firma alterada", if refused { Outcome::Rejected } else { Outcome::Failed }));

    let drifted = link.red.seal(link.integrante, &misquoted(&order, link.red.bic(), link.integrante, now));
    let mismatched = matches!(party.on_report(&drifted, now), Err(Fault::Correlation(_)));
    findings.push(line("correlación distinta", if mismatched { Outcome::Rejected } else { Outcome::Failed }));

    let boundary = now.checked_sub(SignedDuration::from_secs(60)).expect("the clock holds 60 seconds");
    let lapsed = now.checked_sub(SignedDuration::from_secs(61)).expect("the clock holds 61 seconds");
    let window = if party.resolution_open(boundary, now) && !party.resolution_open(lapsed, now) {
        Outcome::Lapsed
    } else {
        Outcome::Failed
    };
    findings.push(line("ventana para confirmar la llave", window));

    for (stage, name) in [
        (Stage::EndToEnd, "tiempo de espera agotado (extremo a extremo)"),
        (Stage::ToSip, "tiempo de espera agotado (envío al SIP)"),
        (Stage::Deliver, "tiempo de espera agotado (entrega a la red beneficiaria)"),
        (Stage::Confirm, "tiempo de espera agotado (confirmación de la red beneficiaria)"),
        (Stage::NotifyOrdering, "tiempo de espera agotado (aviso a la red ordenante)"),
        (Stage::Credit, "tiempo de espera agotado (acreditación)"),
    ] {
        findings.push(line(name, stage_outcome(party, &inbound, stage, now)));
    }
    findings
}

fn stage_outcome(party: &mut impl Integrante, inbound: &Envelope, stage: Stage, now: Timestamp) -> Outcome {
    let limit = StageLimits::default().of(stage);
    let Ok(at_limit) = now.checked_add(limit) else { return Outcome::Failed };
    let Ok(past) = at_limit.checked_add(SignedDuration::from_micros(1)) else { return Outcome::Failed };
    let on_time = party.on_stage(stage, now, at_limit).is_none();
    let late = party.on_stage(stage, now, past);
    let mut outcome = match (on_time, late) {
        (true, Some(got)) if got == stage.notice() => outcome_of(got),
        _ => Outcome::Failed,
    };
    if stage == Stage::Credit && !matches!(party.on_order(inbound, Decision::Credit, past), Ok(Reply::Silent)) {
        outcome = Outcome::Failed;
    }
    outcome
}

fn outcome_of(notice: Notice) -> Outcome {
    match notice {
        Notice::Rejected => Outcome::PaymentRejected,
        Notice::InProcess => Outcome::InProcess,
        Notice::Reconciling => Outcome::Reconciling,
        Notice::PaymentInProcess => Outcome::PaymentInProcess,
        Notice::Unanswered => Outcome::Unanswered,
    }
}

fn cites_order(link: &Link<'_>, reply: &Envelope, order: &CreditTransfer) -> bool {
    let mut trust = Keyring::default();
    if trust.trust(link.integrante, link.integrante_key).is_err() {
        return false;
    }
    let Ok(message) = open_message(&trust, reply) else { return false };
    let Document::StatusReport(report) = &message.document else { return false };
    correlate(order, report).is_ok()
}

fn misquoted(order: &CreditTransfer, from: Bic, to: Bic, now: Timestamp) -> BusinessMessage {
    let id = message_id(now);
    BusinessMessage {
        header: AppHdr { from, to, id: id.clone(), created: now, possible_duplicate: false },
        document: Document::StatusReport(StatusReport {
            msg_id: id,
            created: now,
            beneficiary_agent: order.creditor_agent,
            original_msg_id: "OTRO-MENSAJE".into(),
            original_msg_name: PACS_008.into(),
            original_instr_id: "OTRA-ORDEN".into(),
            original_end_to_end_id: "OTRO-EXTREMO".into(),
            original_debtor_agent: Some(order.debtor_agent),
            status: PaymentState::Acsp,
            reason: None,
            detail: None,
        }),
    }
}

fn line(name: impl Into<String>, outcome: Outcome) -> Finding {
    Finding { name: name.into(), outcome }
}

/// Spanish report a red can read. Every line is one scenario.
pub fn render(findings: &[Finding]) -> String {
    let mut out = String::from(
        "Informe de conformidad del enlace\n\
         \n\
         No está afiliado al Banco Central del Ecuador ni respaldado por él.\n\
         \n\
         Perfil del enlace entre un integrante y su red de pagos. Las reglas citan el Catálogo Operacional. \
         Los 60 segundos son la ventana del cliente para confirmar, después de resolver la llave y antes de que exista la orden. \
         Los 20 segundos son el tope de la orden, desde el inicio hasta la confirmación. \
         Los demás tiempos de cada etapa son parámetros: el catálogo no publica los valores. \
         Si se agota el tiempo de la acreditación, el catálogo no dice qué ocurre y el integrante no confirma.\n\
         \n",
    );
    for finding in findings {
        out.push_str(&finding.name);
        out.push_str(": ");
        out.push_str(finding.outcome.spanish());
        out.push('\n');
    }
    out
}

pub fn passed(findings: &[Finding]) -> bool {
    findings.iter().all(|finding| finding.outcome != Outcome::Failed)
}
