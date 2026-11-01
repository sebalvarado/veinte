//! Encoding of the messages an integrante exchanges with its red.
//!
//! Independent project. Not affiliated with, nor endorsed by, the Banco Central del Ecuador.
//!
//! Llave messages named in Catálogo Operacional §11.1 (`acmt.998`) travel between a red and the
//! SIP. Their fields are not in the published catálogo, so this link does not carry them.

use std::fmt;
use std::str::FromStr;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use quick_xml::XmlVersion;
use quick_xml::escape::escape;
use quick_xml::events::Event;
use veinte_types::{Bic, PaymentId, PaymentState, ReasonCode, ReceivedReason, SCHEME_OFFSET, Usd};

mod profile;
mod sign;

pub use profile::{
    END_TO_END_CEILING, ProfileError, RESOLUTION_CEILING, Stage, StageLimits, connector_may_send, connector_report,
    correlate, overdue, sip_may_send,
};
pub use sign::{Envelope, Keyring, OpenError, Rejected, Signer, open_message};

pub const PACS_008: &str = "pacs.008.001.08";
pub const PACS_002: &str = "pacs.002.001.10";
const HEAD_001: &str = "head.001.001.04";

/// Catálogo Operacional §5, Tabla 10.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    Web,
    App,
}

impl Channel {
    pub const ALL: [Self; 2] = [Self::Web, Self::App];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Web => "0001",
            Self::App => "0002",
        }
    }
}

impl FromStr for Channel {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL.into_iter().find(|c| c.code() == s).ok_or(())
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Catálogo Operacional §5, Tabla 13.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Initiation {
    Llave,
    Credential,
    Qr,
}

impl Initiation {
    pub const ALL: [Self; 3] = [Self::Llave, Self::Credential, Self::Qr];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Llave => "Key",
            Self::Credential => "Cred",
            Self::Qr => "QR",
        }
    }
}

impl FromStr for Initiation {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL.into_iter().find(|c| c.code() == s).ok_or(())
    }
}

impl fmt::Display for Initiation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppHdr {
    pub from: Bic,
    pub to: Bic,
    pub id: String,
    pub created: Timestamp,
    /// Catálogo Operacional §8, Tabla 15 requires this flag on a payment order.
    /// Tabla 17 does not require it on a confirmation, so a confirmation omits it when it is false.
    pub possible_duplicate: bool,
}

/// [assumption] `id_scheme` is `SchmeNm/Cd` from §8, Tabla 16. The catálogo requires the element
/// and does not publish the code list, so any non-empty value is accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Party {
    pub name: String,
    pub id: String,
    pub id_scheme: String,
    pub account: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreditTransfer {
    pub msg_id: String,
    pub created: Timestamp,
    pub instr_id: String,
    pub end_to_end_id: String,
    pub amount: Usd,
    pub instructing_agent: Bic,
    pub instructed_agent: Bic,
    pub debtor: Party,
    pub debtor_agent: Bic,
    pub creditor: Party,
    pub creditor_agent: Bic,
    /// Catálogo Operacional §5, Tabla 10, carried in `InstrForCdtrAgt/InstrInf`.
    pub channel: Channel,
    /// Catálogo Operacional §5, Tabla 13, carried in `InstrForNxtAgt/InstrInf`.
    pub initiation: Initiation,
    /// Catálogo Operacional §8, Tabla 16, `Purp/Prtry`.
    ///
    /// [assumption] The catálogo requires the element and publishes neither a code list nor a length.
    pub purpose: String,
    /// Catálogo Operacional §8, Tabla 16, optional `SplmtryData/Envlp/QrCode`.
    pub qr: Option<String>,
    /// Catálogo Operacional §8, Tabla 16, optional `SplmtryData/Envlp/TimeStamp`.
    pub marked_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    pub msg_id: String,
    pub created: Timestamp,
    /// Catálogo Operacional §8, Tabla 18: `GrpHdr/InstgAgt` is the beneficiary integrante.
    pub beneficiary_agent: Bic,
    /// Catálogo Operacional §8, Tabla 18. [assumption] This repeats `GrpHdr/MsgId` of the order.
    pub original_msg_id: String,
    /// Catálogo Operacional §8, Tabla 18. The order's definition is `pacs.008.001.08` (§8),
    /// not the `pacs.008.004.08` spelling in §11.3.
    pub original_msg_name: String,
    pub original_instr_id: String,
    pub original_end_to_end_id: String,
    pub status: PaymentState,
    pub reason: Option<ReceivedReason>,
    /// Catálogo Operacional §8, Tabla 18: obligatory when `TxSts` is `RJCT`.
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Document {
    CreditTransfer(Box<CreditTransfer>),
    StatusReport(StatusReport),
}

impl Document {
    pub fn definition(&self) -> &'static str {
        match self {
            Self::CreditTransfer(_) => PACS_008,
            Self::StatusReport(_) => PACS_002,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusinessMessage {
    pub header: AppHdr,
    pub document: Document,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    pub reason: ReasonCode,
    pub detail: String,
}

impl DecodeError {
    fn new(reason: ReasonCode, detail: impl Into<String>) -> Self {
        Self { reason, detail: detail.into() }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.reason, self.detail)
    }
}

impl std::error::Error for DecodeError {}

fn datetime(at: Timestamp) -> String {
    at.to_zoned(TimeZone::fixed(SCHEME_OFFSET)).strftime("%Y-%m-%dT%H:%M:%S%.6f%:z").to_string()
}

struct Xml(String);

impl Xml {
    fn open(&mut self, tag: &str) -> &mut Self {
        self.0.push('<');
        self.0.push_str(tag);
        self.0.push('>');
        self
    }

    fn close(&mut self, tag: &str) -> &mut Self {
        self.0.push_str("</");
        self.0.push_str(tag);
        self.0.push('>');
        self
    }

    fn leaf(&mut self, tag: &str, text: &str) -> &mut Self {
        self.open(tag);
        self.0.push_str(&escape(text));
        self.close(tag)
    }

    fn agent(&mut self, tag: &str, bic: Bic) -> &mut Self {
        self.open(tag).open("FinInstnId").leaf("BICFI", bic.as_str()).close("FinInstnId").close(tag)
    }

    fn party(&mut self, tag: &str, account_tag: &str, p: &Party) -> &mut Self {
        self.open(tag).leaf("Nm", &p.name).open("Id").open("PrvtId").open("Othr").leaf("Id", &p.id);
        self.open("SchmeNm").leaf("Cd", &p.id_scheme).close("SchmeNm");
        self.close("Othr").close("PrvtId").close("Id").close(tag);
        self.open(account_tag)
            .open("Id")
            .open("Othr")
            .leaf("Id", &p.account)
            .close("Othr")
            .close("Id")
            .close(account_tag)
    }
}

pub fn encode(m: &BusinessMessage) -> Vec<u8> {
    let h = &m.header;
    let mut x = Xml(String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<BizMsg>"));
    x.0.push_str(&format!("<AppHdr xmlns=\"urn:iso:std:iso:20022:tech:xsd:{HEAD_001}\">"));
    x.open("Fr")
        .open("FIId")
        .open("FinInstnId")
        .leaf("BICFI", h.from.as_str())
        .close("FinInstnId")
        .close("FIId")
        .close("Fr");
    x.open("To")
        .open("FIId")
        .open("FinInstnId")
        .leaf("BICFI", h.to.as_str())
        .close("FinInstnId")
        .close("FIId")
        .close("To");
    x.leaf("BizMsgIdr", &h.id).leaf("MsgDefIdr", m.document.definition()).leaf("CreDt", &datetime(h.created));
    if h.possible_duplicate || matches!(m.document, Document::CreditTransfer(_)) {
        x.leaf("PssblDplct", if h.possible_duplicate { "true" } else { "false" });
    }
    x.close("AppHdr");
    x.0.push_str(&format!("<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:{}\">", m.document.definition()));
    match &m.document {
        Document::CreditTransfer(t) => {
            x.open("FIToFICstmrCdtTrf").open("GrpHdr").leaf("MsgId", &t.msg_id).leaf("CreDtTm", &datetime(t.created));
            x.leaf("NbOfTxs", "1").open("SttlmInf").leaf("SttlmMtd", "CLRG").close("SttlmInf").close("GrpHdr");
            x.open("CdtTrfTxInf").open("PmtId").leaf("InstrId", &t.instr_id).leaf("EndToEndId", &t.end_to_end_id);
            x.close("PmtId");
            x.0.push_str(&format!("<IntrBkSttlmAmt Ccy=\"USD\">{}</IntrBkSttlmAmt>", t.amount));
            x.leaf("ChrgBr", "DEBT").agent("InstgAgt", t.instructing_agent).agent("InstdAgt", t.instructed_agent);
            x.party("Dbtr", "DbtrAcct", &t.debtor).agent("DbtrAgt", t.debtor_agent);
            x.agent("CdtrAgt", t.creditor_agent).party("Cdtr", "CdtrAcct", &t.creditor);
            x.open("InstrForCdtrAgt").leaf("InstrInf", t.channel.code()).close("InstrForCdtrAgt");
            x.open("InstrForNxtAgt").leaf("InstrInf", t.initiation.code()).close("InstrForNxtAgt");
            x.open("Purp").leaf("Prtry", &t.purpose).close("Purp");
            if t.qr.is_some() || t.marked_at.is_some() {
                x.open("SplmtryData").open("Envlp");
                if let Some(qr) = &t.qr {
                    x.leaf("QrCode", qr);
                }
                if let Some(at) = t.marked_at {
                    x.leaf("TimeStamp", &datetime(at));
                }
                x.close("Envlp").close("SplmtryData");
            }
            x.close("CdtTrfTxInf").close("FIToFICstmrCdtTrf");
        }
        Document::StatusReport(r) => {
            x.open("FIToFIPmtStsRpt").open("GrpHdr").leaf("MsgId", &r.msg_id).leaf("CreDtTm", &datetime(r.created));
            x.agent("InstgAgt", r.beneficiary_agent).close("GrpHdr");
            x.open("OrgnlGrpInfAndSts")
                .leaf("OrgnlMsgId", &r.original_msg_id)
                .leaf("OrgnlMsgNmId", &r.original_msg_name);
            x.close("OrgnlGrpInfAndSts").open("TxInfAndSts").leaf("OrgnlInstrId", &r.original_instr_id);
            x.leaf("OrgnlEndToEndId", &r.original_end_to_end_id).leaf("TxSts", r.status.code());
            if let Some(reason) = &r.reason {
                x.open("StsRsnInf").open("Rsn").leaf("Cd", reason.code()).close("Rsn");
                if let Some(detail) = &r.detail {
                    x.leaf("AddtlInf", detail);
                }
                x.close("StsRsnInf");
            }
            x.close("TxInfAndSts").close("FIToFIPmtStsRpt");
        }
    }
    x.close("Document").close("BizMsg");
    x.0.push('\n');
    x.0.into_bytes()
}

pub fn pretty(bytes: &[u8]) -> String {
    let mut reader = quick_xml::Reader::from_reader(bytes);
    let mut writer = quick_xml::Writer::new_with_indent(Vec::new(), b' ', 2);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) | Err(_) => break,
            Ok(Event::Text(t)) if t.xml10_content().trim().is_empty() => {}
            Ok(e) => {
                if writer.write_event(e).is_err() {
                    break;
                }
            }
        }
        buf.clear();
    }
    let mut out = String::from_utf8(writer.into_inner()).unwrap_or_default();
    out.push('\n');
    out
}

struct Fields(Vec<(String, String)>);

impl Fields {
    fn read(bytes: &[u8]) -> Result<Self, DecodeError> {
        let syntax = |e: &dyn fmt::Display| DecodeError::new(ReasonCode::Ff02, e.to_string());
        let mut reader = quick_xml::Reader::from_reader(bytes);
        let mut path: Vec<String> = Vec::new();
        let mut fields = Vec::new();
        let mut text = String::new();
        let mut buf = Vec::new();
        fn flush(fields: &mut Vec<(String, String)>, path: &[String], text: &mut String) {
            if !text.trim().is_empty() {
                fields.push((path.join("/"), std::mem::take(text)));
            }
            text.clear();
        }
        loop {
            match reader.read_event_into(&mut buf).map_err(|e| syntax(&e))? {
                Event::Start(e) => {
                    flush(&mut fields, &path, &mut text);
                    path.push(e.local_name().as_ref().to_owned());
                    for a in e.attributes() {
                        let a = a.map_err(|e| syntax(&e))?;
                        if a.key.as_ref() == "xmlns" {
                            continue;
                        }
                        let value = a.normalized_value(XmlVersion::Implicit1_0).map_err(|e| syntax(&e))?;
                        let key = a.key.local_name();
                        fields.push((format!("{}@{}", path.join("/"), key.as_ref()), value.into_owned()));
                    }
                }
                Event::End(_) => {
                    flush(&mut fields, &path, &mut text);
                    path.pop();
                }
                Event::Text(t) => text.push_str(&t.xml10_content()),
                Event::GeneralRef(r) => match r.resolve_char_ref().map_err(|e| syntax(&e))? {
                    Some(c) => text.push(c),
                    None => {
                        let entity = format!("&{};", r.xml10_content());
                        text.push_str(&quick_xml::escape::unescape(&entity).map_err(|e| syntax(&e))?);
                    }
                },
                Event::Eof => break,
                Event::Empty(_) => return Err(DecodeError::new(ReasonCode::Ff02, "empty element")),
                _ => {}
            }
            buf.clear();
        }
        if !path.is_empty() {
            return Err(DecodeError::new(ReasonCode::Ff02, "unclosed element"));
        }
        Ok(Self(fields))
    }

    fn opt(&self, path: &str) -> Option<&str> {
        self.0.iter().find(|(p, _)| p == path).map(|(_, v)| v.as_str())
    }

    fn get(&self, path: &str) -> Result<&str, DecodeError> {
        self.opt(path).ok_or_else(|| DecodeError::new(ReasonCode::Ch21, format!("missing {path}")))
    }

    fn bic(&self, path: &str) -> Result<Bic, DecodeError> {
        self.get(path)?
            .parse()
            .map_err(|e: veinte_types::InvalidValue| DecodeError::new(ReasonCode::Ch16, format!("{path}: {e}")))
    }

    fn time(&self, path: &str) -> Result<Timestamp, DecodeError> {
        self.get(path)?.parse().map_err(|_| DecodeError::new(ReasonCode::Dt02, format!("{path}: not a date-time")))
    }

    fn party(&self, base: &str, party: &str, account: &str) -> Result<Party, DecodeError> {
        let id_scheme = self.get(&format!("{base}/{party}/Id/PrvtId/Othr/SchmeNm/Cd"))?;
        if id_scheme.is_empty() {
            return Err(DecodeError::new(ReasonCode::Ch21, "missing identification scheme"));
        }
        Ok(Party {
            name: self.get(&format!("{base}/{party}/Nm"))?.to_owned(),
            id: self.get(&format!("{base}/{party}/Id/PrvtId/Othr/Id"))?.to_owned(),
            id_scheme: id_scheme.to_owned(),
            account: self.get(&format!("{base}/{account}/Id/Othr/Id"))?.to_owned(),
        })
    }

    fn duplicate(&self, path: &str, required: bool) -> Result<bool, DecodeError> {
        match self.opt(path) {
            None if required => Err(DecodeError::new(ReasonCode::Ch21, format!("missing {path}"))),
            None => Ok(false),
            Some("true" | "1" | "True") => Ok(true),
            Some("false" | "0" | "False") => Ok(false),
            Some(other) => Err(DecodeError::new(ReasonCode::Ch16, format!("duplicate flag {other:?}"))),
        }
    }
}

pub fn decode(bytes: &[u8]) -> Result<BusinessMessage, DecodeError> {
    let f = Fields::read(bytes)?;
    let h = "BizMsg/AppHdr";
    let definition = f.get(&format!("{h}/MsgDefIdr"))?;
    let header = AppHdr {
        from: f.bic(&format!("{h}/Fr/FIId/FinInstnId/BICFI"))?,
        to: f.bic(&format!("{h}/To/FIId/FinInstnId/BICFI"))?,
        id: f.get(&format!("{h}/BizMsgIdr"))?.to_owned(),
        created: f.time(&format!("{h}/CreDt"))?,
        possible_duplicate: f.duplicate(&format!("{h}/PssblDplct"), definition == PACS_008)?,
    };
    let document = match definition {
        PACS_008 => {
            let g = "BizMsg/Document/FIToFICstmrCdtTrf/GrpHdr";
            let t = "BizMsg/Document/FIToFICstmrCdtTrf/CdtTrfTxInf";
            if f.get(&format!("{g}/NbOfTxs"))? != "1" {
                return Err(DecodeError::new(ReasonCode::Ds0k, "a message carries one payment"));
            }
            // [assumption] Tabla 16 requires the settlement method and does not name the code.
            // §9 settles by deferred netting, which this profile writes as CLRG.
            if f.get(&format!("{g}/SttlmInf/SttlmMtd"))? != "CLRG" {
                return Err(DecodeError::new(ReasonCode::Ch16, "the settlement method is CLRG"));
            }
            if f.get(&format!("{t}/IntrBkSttlmAmt@Ccy"))? != "USD" {
                return Err(DecodeError::new(ReasonCode::Am11, "the currency is USD"));
            }
            // [assumption] §8 says the charge bearer defaults to DEBT and publishes no other value.
            if f.get(&format!("{t}/ChrgBr"))? != "DEBT" {
                return Err(DecodeError::new(ReasonCode::Ch16, "the charge bearer is DEBT"));
            }
            let amount_text = f.get(&format!("{t}/IntrBkSttlmAmt"))?;
            let amount: Usd = amount_text
                .parse()
                .map_err(|_| DecodeError::new(ReasonCode::Ch20, format!("amount {amount_text:?}")))?;
            if amount == Usd::ZERO {
                return Err(DecodeError::new(ReasonCode::Am01, "the amount is zero"));
            }
            let debtor_agent = f.bic(&format!("{t}/DbtrAgt/FinInstnId/BICFI"))?;
            let instr_id = f.get(&format!("{t}/PmtId/InstrId"))?.to_owned();
            // Catálogo Operacional §7 Tabla 14 and §8 Tabla 16: InstrId is the payment identifier.
            if PaymentId::new(debtor_agent, &instr_id).is_err() {
                return Err(DecodeError::new(ReasonCode::Ms01, format!("payment id {instr_id:?}")));
            }
            let channel = f.get(&format!("{t}/InstrForCdtrAgt/InstrInf"))?;
            let initiation = f.get(&format!("{t}/InstrForNxtAgt/InstrInf"))?;
            let marked_at = match f.opt(&format!("{t}/SplmtryData/Envlp/TimeStamp")) {
                Some(_) => Some(f.time(&format!("{t}/SplmtryData/Envlp/TimeStamp"))?),
                None => None,
            };
            Document::CreditTransfer(Box::new(CreditTransfer {
                msg_id: f.get(&format!("{g}/MsgId"))?.to_owned(),
                created: f.time(&format!("{g}/CreDtTm"))?,
                instr_id,
                end_to_end_id: f.get(&format!("{t}/PmtId/EndToEndId"))?.to_owned(),
                amount,
                instructing_agent: f.bic(&format!("{t}/InstgAgt/FinInstnId/BICFI"))?,
                instructed_agent: f.bic(&format!("{t}/InstdAgt/FinInstnId/BICFI"))?,
                debtor: f.party(t, "Dbtr", "DbtrAcct")?,
                debtor_agent,
                creditor: f.party(t, "Cdtr", "CdtrAcct")?,
                creditor_agent: f.bic(&format!("{t}/CdtrAgt/FinInstnId/BICFI"))?,
                channel: channel
                    .parse()
                    .map_err(|_| DecodeError::new(ReasonCode::Ch16, format!("channel {channel:?}")))?,
                initiation: initiation
                    .parse()
                    .map_err(|_| DecodeError::new(ReasonCode::Ch16, format!("initiation {initiation:?}")))?,
                purpose: f.get(&format!("{t}/Purp/Prtry"))?.to_owned(),
                qr: f.opt(&format!("{t}/SplmtryData/Envlp/QrCode")).map(str::to_owned),
                marked_at,
            }))
        }
        PACS_002 => {
            let g = "BizMsg/Document/FIToFIPmtStsRpt/GrpHdr";
            let o = "BizMsg/Document/FIToFIPmtStsRpt/OrgnlGrpInfAndSts";
            let t = "BizMsg/Document/FIToFIPmtStsRpt/TxInfAndSts";
            let status_text = f.get(&format!("{t}/TxSts"))?;
            let status: PaymentState = status_text
                .parse()
                .map_err(|_| DecodeError::new(ReasonCode::Ch16, format!("status {status_text:?}")))?;
            let reason = f.opt(&format!("{t}/StsRsnInf/Rsn/Cd"));
            let detail = f.opt(&format!("{t}/StsRsnInf/AddtlInf")).map(str::to_owned);
            let rejected = status == PaymentState::Rjct;
            if rejected && reason.is_none() {
                return Err(DecodeError::new(ReasonCode::Ch21, "a rejection needs a reason"));
            }
            if rejected && detail.is_none() {
                return Err(DecodeError::new(ReasonCode::Ch21, "a rejection needs additional information"));
            }
            // [assumption] Tabla 18 requires the reason only when the status is RJCT.
            if !rejected && (reason.is_some() || detail.is_some()) {
                return Err(DecodeError::new(ReasonCode::Ch16, "only a rejection carries a reason"));
            }
            let original_msg_name = f.get(&format!("{o}/OrgnlMsgNmId"))?;
            if original_msg_name != PACS_008 {
                return Err(DecodeError::new(ReasonCode::Ch16, format!("original message {original_msg_name:?}")));
            }
            Document::StatusReport(StatusReport {
                msg_id: f.get(&format!("{g}/MsgId"))?.to_owned(),
                created: f.time(&format!("{g}/CreDtTm"))?,
                beneficiary_agent: f.bic(&format!("{g}/InstgAgt/FinInstnId/BICFI"))?,
                original_msg_id: f.get(&format!("{o}/OrgnlMsgId"))?.to_owned(),
                original_msg_name: original_msg_name.to_owned(),
                original_instr_id: f.get(&format!("{t}/OrgnlInstrId"))?.to_owned(),
                original_end_to_end_id: f.get(&format!("{t}/OrgnlEndToEndId"))?.to_owned(),
                status,
                reason: reason
                    .map(|r| r.parse().map_err(|_| DecodeError::new(ReasonCode::Ch16, format!("reason {r:?}"))))
                    .transpose()?,
                detail,
            })
        }
        other => return Err(DecodeError::new(ReasonCode::Ch16, format!("unsupported message {other:?}"))),
    };
    if header.from == header.to {
        return Err(DecodeError::new(ReasonCode::Ch16, "sender and receiver are the same"));
    }
    Ok(BusinessMessage { header, document })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use jiff::Timestamp;

    pub(crate) mod sample {
        use super::*;

        fn bic(s: &str) -> Bic {
            s.parse().unwrap()
        }

        fn at() -> Timestamp {
            "2026-10-12T14:00:00.250Z".parse().unwrap()
        }

        fn party(name: &str, id: &str, account: &str) -> Party {
            Party { name: name.into(), id: id.into(), id_scheme: "NIDN".into(), account: account.into() }
        }

        pub(crate) fn order() -> (BusinessMessage, Document) {
            let document = Document::CreditTransfer(Box::new(CreditTransfer {
                msg_id: "M0000000000000000000000000000001".into(),
                created: at(),
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
            }));
            let message = BusinessMessage {
                header: AppHdr {
                    from: bic("RALFECEQ"),
                    to: bic("SIPCECEQ"),
                    id: "M0000000000000000000000000000001".into(),
                    created: at(),
                    possible_duplicate: false,
                },
                document: document.clone(),
            };
            (message, document)
        }

        pub(crate) fn status(reason: Option<ReceivedReason>) -> (BusinessMessage, Document) {
            let document = Document::StatusReport(StatusReport {
                msg_id: "M0000000000000000000000000000002".into(),
                created: at(),
                beneficiary_agent: bic("CBETECEQ"),
                original_msg_id: "M0000000000000000000000000000001".into(),
                original_msg_name: PACS_008.into(),
                original_instr_id: "BALFECEQ12102026090000000001".into(),
                original_end_to_end_id: "BALFECEQ12102026090000000001".into(),
                status: if reason.is_some() { PaymentState::Rjct } else { PaymentState::Acsp },
                reason,
                detail: reason.map(|r| r.to_string()),
            });
            let message = BusinessMessage {
                header: AppHdr {
                    from: bic("SIPCECEQ"),
                    to: bic("RALFECEQ"),
                    id: "M0000000000000000000000000000002".into(),
                    created: at(),
                    possible_duplicate: true,
                },
                document: document.clone(),
            };
            (message, document)
        }
    }
}
