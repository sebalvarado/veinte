use std::fmt;

use caudal_types::{Bic, PaymentState, ReasonCode, ReceivedReason, SCHEME_OFFSET, Usd};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use quick_xml::XmlVersion;
use quick_xml::escape::escape;
use quick_xml::events::Event;

pub const PACS_008: &str = "pacs.008.001.08";
pub const PACS_002: &str = "pacs.002.001.10";
const HEAD_001: &str = "head.001.001.04";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppHdr {
    pub from: Bic,
    pub to: Bic,
    pub id: String,
    pub created: Timestamp,
    pub possible_duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Party {
    pub name: String,
    pub id: String,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    pub msg_id: String,
    pub created: Timestamp,
    pub original_instr_id: String,
    pub original_debtor_agent: Bic,
    pub status: PaymentState,
    pub reason: Option<ReceivedReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Document {
    CreditTransfer(CreditTransfer),
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
    if h.possible_duplicate {
        x.leaf("PssblDplct", "true");
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
            x.close("CdtTrfTxInf").close("FIToFICstmrCdtTrf");
        }
        Document::StatusReport(r) => {
            x.open("FIToFIPmtStsRpt").open("GrpHdr").leaf("MsgId", &r.msg_id).leaf("CreDtTm", &datetime(r.created));
            x.close("GrpHdr").open("TxInfAndSts").leaf("OrgnlInstrId", &r.original_instr_id);
            x.leaf("TxSts", r.status.code());
            if let Some(reason) = &r.reason {
                x.open("StsRsnInf").open("Rsn").leaf("Cd", reason.code()).close("Rsn").close("StsRsnInf");
            }
            x.open("OrgnlTxRef").agent("DbtrAgt", r.original_debtor_agent).close("OrgnlTxRef");
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
            .map_err(|e: caudal_types::InvalidValue| DecodeError::new(ReasonCode::Ch16, format!("{path}: {e}")))
    }

    fn time(&self, path: &str) -> Result<Timestamp, DecodeError> {
        self.get(path)?.parse().map_err(|_| DecodeError::new(ReasonCode::Dt02, format!("{path}: not a date-time")))
    }

    fn party(&self, base: &str, party: &str, account: &str) -> Result<Party, DecodeError> {
        Ok(Party {
            name: self.get(&format!("{base}/{party}/Nm"))?.to_owned(),
            id: self.get(&format!("{base}/{party}/Id/PrvtId/Othr/Id"))?.to_owned(),
            account: self.get(&format!("{base}/{account}/Id/Othr/Id"))?.to_owned(),
        })
    }
}

pub fn decode(bytes: &[u8]) -> Result<BusinessMessage, DecodeError> {
    let f = Fields::read(bytes)?;
    let h = "BizMsg/AppHdr";
    let header = AppHdr {
        from: f.bic(&format!("{h}/Fr/FIId/FinInstnId/BICFI"))?,
        to: f.bic(&format!("{h}/To/FIId/FinInstnId/BICFI"))?,
        id: f.get(&format!("{h}/BizMsgIdr"))?.to_owned(),
        created: f.time(&format!("{h}/CreDt"))?,
        possible_duplicate: f.opt(&format!("{h}/PssblDplct")) == Some("true"),
    };
    let document = match f.get(&format!("{h}/MsgDefIdr"))? {
        PACS_008 => {
            let g = "BizMsg/Document/FIToFICstmrCdtTrf/GrpHdr";
            let t = "BizMsg/Document/FIToFICstmrCdtTrf/CdtTrfTxInf";
            if f.get(&format!("{t}/IntrBkSttlmAmt@Ccy"))? != "USD" {
                return Err(DecodeError::new(ReasonCode::Am11, "the currency is USD"));
            }
            let amount = f.get(&format!("{t}/IntrBkSttlmAmt"))?;
            Document::CreditTransfer(CreditTransfer {
                msg_id: f.get(&format!("{g}/MsgId"))?.to_owned(),
                created: f.time(&format!("{g}/CreDtTm"))?,
                instr_id: f.get(&format!("{t}/PmtId/InstrId"))?.to_owned(),
                end_to_end_id: f.get(&format!("{t}/PmtId/EndToEndId"))?.to_owned(),
                amount: amount.parse().map_err(|_| DecodeError::new(ReasonCode::Ch20, format!("amount {amount:?}")))?,
                instructing_agent: f.bic(&format!("{t}/InstgAgt/FinInstnId/BICFI"))?,
                instructed_agent: f.bic(&format!("{t}/InstdAgt/FinInstnId/BICFI"))?,
                debtor: f.party(t, "Dbtr", "DbtrAcct")?,
                debtor_agent: f.bic(&format!("{t}/DbtrAgt/FinInstnId/BICFI"))?,
                creditor: f.party(t, "Cdtr", "CdtrAcct")?,
                creditor_agent: f.bic(&format!("{t}/CdtrAgt/FinInstnId/BICFI"))?,
            })
        }
        PACS_002 => {
            let g = "BizMsg/Document/FIToFIPmtStsRpt/GrpHdr";
            let t = "BizMsg/Document/FIToFIPmtStsRpt/TxInfAndSts";
            let status = f.get(&format!("{t}/TxSts"))?;
            let reason = f.opt(&format!("{t}/StsRsnInf/Rsn/Cd"));
            Document::StatusReport(StatusReport {
                msg_id: f.get(&format!("{g}/MsgId"))?.to_owned(),
                created: f.time(&format!("{g}/CreDtTm"))?,
                original_instr_id: f.get(&format!("{t}/OrgnlInstrId"))?.to_owned(),
                original_debtor_agent: f.bic(&format!("{t}/OrgnlTxRef/DbtrAgt/FinInstnId/BICFI"))?,
                status: status.parse().map_err(|_| DecodeError::new(ReasonCode::Ch16, format!("status {status:?}")))?,
                reason: reason
                    .map(|r| r.parse().map_err(|_| DecodeError::new(ReasonCode::Ch16, format!("reason {r:?}"))))
                    .transpose()?,
            })
        }
        other => return Err(DecodeError::new(ReasonCode::Ch16, format!("unsupported message {other:?}"))),
    };
    if header.from == header.to {
        return Err(DecodeError::new(ReasonCode::Ch16, "sender and receiver are the same"));
    }
    Ok(BusinessMessage { header, document })
}
