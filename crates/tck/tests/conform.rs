use jiff::Timestamp;
use veinte_iso20022::{
    AppHdr, BusinessMessage, CreditTransfer, Document, Envelope, Keyring, Notice, PACS_008, Stage, StageLimits,
    StatusReport, example_order, open_message,
};
use veinte_tck::{Decision, Fault, Finding, Integrante, Link, Outcome, Reference, Reply, drive, passed, render};
use veinte_types::{Bic, PaymentState, ReasonCode, ReceivedReason};

/// Tabla 12, written here and not taken from `integrante_may_send`.
/// IR Beneficiario: AC04, AC01, AC06, AC05, AC07. Todos: DT01.
const BENEFICIARY_INTEGRANTE: &[&str] = &["AC04", "AC01", "AC06", "AC05", "AC07", "DT01"];

/// Lines the report must contain. Written by hand, not copied from a regenerated file.
const VERDICTS: &[&str] = &[
    "No está afiliado al Banco Central del Ecuador ni respaldado por él.",
    "orden pacs.008: aceptada",
    "confirmación pacs.002: aceptada",
    "firma alterada: rechazada",
    "correlación distinta: rechazada",
    "ventana para confirmar la llave: vencida",
    "tiempo de espera agotado (extremo a extremo): pago rechazado",
    "tiempo de espera agotado (envío al SIP): en proceso",
    "tiempo de espera agotado (entrega a la red beneficiaria): en proceso",
    "tiempo de espera agotado (confirmación de la red beneficiaria): por conciliar",
    "tiempo de espera agotado (aviso a la red ordenante): pago en proceso",
    "tiempo de espera agotado (acreditación): sin confirmación",
];

fn now() -> Timestamp {
    "2026-10-12T14:00:00.250Z".parse().unwrap()
}

fn link_for<'a>(integrante: &SignerBic, red: &'a veinte_iso20022::Signer) -> Link<'a> {
    Link { red, integrante: integrante.bic, integrante_key: integrante.key, now: now() }
}

struct SignerBic {
    bic: Bic,
    key: [u8; 32],
}

fn reference() -> (Reference, veinte_iso20022::Signer) {
    let integrante = veinte_iso20022::Signer::generate("CBETECEQ".parse().unwrap()).unwrap();
    let red = veinte_iso20022::Signer::generate("RBETECEQ".parse().unwrap()).unwrap();
    let party = Reference::new(integrante, red.bic(), red.verifying_key(), StageLimits::default()).unwrap();
    (party, red)
}

fn code_findings(party: &mut impl Integrante, link: &Link<'_>) -> Vec<Finding> {
    ReasonCode::ALL
        .into_iter()
        .map(|code| Finding { name: format!("código {code}"), outcome: judge(party, link, code) })
        .collect()
}

fn judge(party: &mut impl Integrante, link: &Link<'_>, code: ReasonCode) -> Outcome {
    let allowed = BENEFICIARY_INTEGRANTE.contains(&code.code());
    let order = example_order(link.red.bic(), link.integrante, link.now);
    let envelope = link.red.seal(link.integrante, &order);
    match party.on_order(&envelope, Decision::Reject(code), link.now) {
        Ok(Reply::Confirmation(reply)) => {
            let mut trust = Keyring::default();
            if trust.trust(link.integrante, link.integrante_key).is_err() {
                return Outcome::Failed;
            }
            let Ok(message) = open_message(&trust, &reply) else { return Outcome::Failed };
            let Document::StatusReport(report) = &message.document else { return Outcome::Failed };
            if allowed && report.reason == Some(ReceivedReason::Listed(code)) {
                Outcome::Allowed
            } else {
                Outcome::Failed
            }
        }
        Err(Fault::Profile(_)) if !allowed => Outcome::NotAllowed,
        _ => Outcome::Failed,
    }
}

#[test]
fn the_reference_integrante_passes_every_scenario() {
    let (mut party, red) = reference();
    let held = SignerBic { bic: party.bic(), key: party.verifying_key() };
    let link = link_for(&held, &red);
    let mut findings = drive(&mut party, &link);
    findings.extend(code_findings(&mut party, &link));
    let report = render(&findings);
    // The file is the full text. The lines in VERDICTS and the code list below are separate
    // expectations, written here, so a regenerated file is not the only check.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/informe.txt");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &report).unwrap();
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{} missing", path.display()));
    assert!(passed(&findings), "{report}");
    assert_eq!(report, expected);
    for line in VERDICTS {
        assert!(report.contains(line), "missing {line}\n{report}");
    }
    assert!(!report.contains("timeout"), "{report}");
    for code in ReasonCode::ALL {
        let outcome = if BENEFICIARY_INTEGRANTE.contains(&code.code()) { "permitido" } else { "no permitido" };
        let line = format!("código {code}: {outcome}");
        assert!(report.contains(&line), "missing {line}\n{report}");
    }
}

#[test]
fn an_integrante_that_accepts_a_bad_signature_fails() {
    let (inner, red) = reference();
    let mut party = AcceptsBadSignature(inner);
    let held = SignerBic { bic: party.0.bic(), key: party.0.verifying_key() };
    let findings = drive(&mut party, &link_for(&held, &red));
    assert_eq!(outcome(&findings, "firma alterada"), Outcome::Failed);
    assert!(!passed(&findings));
}

#[test]
fn an_integrante_that_misquotes_the_order_fails() {
    let integrante = veinte_iso20022::Signer::generate("CBETECEQ".parse().unwrap()).unwrap();
    let red = veinte_iso20022::Signer::generate("RBETECEQ".parse().unwrap()).unwrap();
    let inner = Reference::new(integrante.clone(), red.bic(), red.verifying_key(), StageLimits::default()).unwrap();
    let mut party = Misquotes { inner, signer: integrante, red: red.bic() };
    let held = SignerBic { bic: party.signer.bic(), key: party.signer.verifying_key() };
    let findings = drive(&mut party, &link_for(&held, &red));
    assert_eq!(outcome(&findings, "confirmación pacs.002"), Outcome::Failed);
    assert_eq!(outcome(&findings, "correlación distinta"), Outcome::Failed);
    assert!(!passed(&findings));
}

#[test]
fn an_integrante_that_sends_an_sip_code_fails() {
    let integrante = veinte_iso20022::Signer::generate("CBETECEQ".parse().unwrap()).unwrap();
    let red = veinte_iso20022::Signer::generate("RBETECEQ".parse().unwrap()).unwrap();
    let mut peers = Keyring::default();
    peers.trust(red.bic(), red.verifying_key()).unwrap();
    let inner = Reference::new(integrante.clone(), red.bic(), red.verifying_key(), StageLimits::default()).unwrap();
    let mut party = SendsSipCode { inner, signer: integrante, red: red.bic(), peers };
    let held = SignerBic { bic: party.signer.bic(), key: party.signer.verifying_key() };
    let link = link_for(&held, &red);
    let findings = code_findings(&mut party, &link);
    assert_eq!(outcome(&findings, "código AC04"), Outcome::Failed);
    assert_eq!(outcome(&findings, "código AM04"), Outcome::Failed);
    assert!(!passed(&findings));
}

#[test]
fn an_integrante_that_ignores_a_time_limit_fails() {
    let (inner, red) = reference();
    let started = now();
    let mut party = IgnoresLimit { inner, started };
    let held = SignerBic { bic: party.inner.bic(), key: party.inner.verifying_key() };
    let findings = drive(&mut party, &link_for(&held, &red));
    assert_eq!(outcome(&findings, "tiempo de espera agotado (envío al SIP)"), Outcome::Failed);
    assert_eq!(outcome(&findings, "tiempo de espera agotado (acreditación)"), Outcome::Failed);
    assert!(!passed(&findings));
}

fn outcome(findings: &[Finding], name: &str) -> Outcome {
    findings.iter().find(|finding| finding.name == name).unwrap_or_else(|| panic!("missing {name}")).outcome
}

struct AcceptsBadSignature(Reference);

impl Integrante for AcceptsBadSignature {
    fn on_order(&mut self, envelope: &Envelope, decision: Decision, now: Timestamp) -> Result<Reply, Fault> {
        match self.0.on_order(envelope, decision, now) {
            Err(Fault::Signature(_)) => Ok(Reply::Silent),
            other => other,
        }
    }

    fn on_report(&mut self, envelope: &Envelope, now: Timestamp) -> Result<(), Fault> {
        self.0.on_report(envelope, now)
    }

    fn on_stage(&self, stage: Stage, started: Timestamp, now: Timestamp) -> Option<Notice> {
        self.0.on_stage(stage, started, now)
    }

    fn resolution_open(&self, resolved: Timestamp, now: Timestamp) -> bool {
        self.0.resolution_open(resolved, now)
    }
}

struct Misquotes {
    inner: Reference,
    signer: veinte_iso20022::Signer,
    red: Bic,
}

impl Misquotes {
    fn twist(&self, envelope: Envelope) -> Envelope {
        let mut trust = Keyring::default();
        trust.trust(self.signer.bic(), self.signer.verifying_key()).unwrap();
        let mut message = open_message(&trust, &envelope).unwrap();
        if let Document::StatusReport(report) = &mut message.document {
            report.original_msg_id = "OTRO-MENSAJE".into();
            report.original_instr_id = "OTRA-ORDEN".into();
            report.original_end_to_end_id = "OTRO-EXTREMO".into();
        }
        self.signer.seal(self.red, &message)
    }
}

impl Integrante for Misquotes {
    fn on_order(&mut self, envelope: &Envelope, decision: Decision, now: Timestamp) -> Result<Reply, Fault> {
        match self.inner.on_order(envelope, decision, now)? {
            Reply::Confirmation(reply) => Ok(Reply::Confirmation(self.twist(reply))),
            silent => Ok(silent),
        }
    }

    fn on_report(&mut self, _envelope: &Envelope, _now: Timestamp) -> Result<(), Fault> {
        Ok(())
    }

    fn on_stage(&self, stage: Stage, started: Timestamp, now: Timestamp) -> Option<Notice> {
        self.inner.on_stage(stage, started, now)
    }

    fn resolution_open(&self, resolved: Timestamp, now: Timestamp) -> bool {
        self.inner.resolution_open(resolved, now)
    }
}

struct SendsSipCode {
    inner: Reference,
    signer: veinte_iso20022::Signer,
    red: Bic,
    peers: Keyring,
}

impl SendsSipCode {
    fn seal_am04(&self, order: &CreditTransfer, now: Timestamp) -> Envelope {
        let message = BusinessMessage {
            header: AppHdr {
                from: self.signer.bic(),
                to: self.red,
                id: "C1".into(),
                created: now,
                possible_duplicate: false,
            },
            document: Document::StatusReport(StatusReport {
                msg_id: "C1".into(),
                created: now,
                beneficiary_agent: order.creditor_agent,
                original_msg_id: order.msg_id.clone(),
                original_msg_name: PACS_008.into(),
                original_instr_id: order.instr_id.clone(),
                original_end_to_end_id: order.end_to_end_id.clone(),
                original_debtor_agent: Some(order.debtor_agent),
                status: PaymentState::Rjct,
                reason: Some(ReceivedReason::Listed(ReasonCode::Am04)),
                detail: Some(ReasonCode::Am04.description().to_owned()),
            }),
        };
        self.signer.seal(self.red, &message)
    }
}

impl Integrante for SendsSipCode {
    fn on_order(&mut self, envelope: &Envelope, decision: Decision, now: Timestamp) -> Result<Reply, Fault> {
        if let Decision::Reject(_) = decision {
            let message = open_message(&self.peers, envelope).map_err(Fault::Signature)?;
            let Document::CreditTransfer(order) = &message.document else {
                return self.inner.on_order(envelope, decision, now);
            };
            return Ok(Reply::Confirmation(self.seal_am04(order, now)));
        }
        self.inner.on_order(envelope, decision, now)
    }

    fn on_report(&mut self, envelope: &Envelope, now: Timestamp) -> Result<(), Fault> {
        self.inner.on_report(envelope, now)
    }

    fn on_stage(&self, stage: Stage, started: Timestamp, now: Timestamp) -> Option<Notice> {
        self.inner.on_stage(stage, started, now)
    }

    fn resolution_open(&self, resolved: Timestamp, now: Timestamp) -> bool {
        self.inner.resolution_open(resolved, now)
    }
}

struct IgnoresLimit {
    inner: Reference,
    started: Timestamp,
}

impl Integrante for IgnoresLimit {
    fn on_order(&mut self, envelope: &Envelope, decision: Decision, _now: Timestamp) -> Result<Reply, Fault> {
        self.inner.on_order(envelope, decision, self.started)
    }

    fn on_report(&mut self, envelope: &Envelope, now: Timestamp) -> Result<(), Fault> {
        self.inner.on_report(envelope, now)
    }

    fn on_stage(&self, _stage: Stage, _started: Timestamp, _now: Timestamp) -> Option<Notice> {
        None
    }

    fn resolution_open(&self, resolved: Timestamp, now: Timestamp) -> bool {
        self.inner.resolution_open(resolved, now)
    }
}
