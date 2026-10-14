use std::path::PathBuf;

use caudal_iso20022::{AppHdr, BusinessMessage, CreditTransfer, Document, Party, StatusReport, decode, encode, pretty};
use caudal_types::{Bic, PaymentState, ReasonCode, ReceivedReason, Usd};
use jiff::Timestamp;
use proptest::prelude::*;

fn bic(s: &str) -> Bic {
    s.parse().unwrap()
}

fn at() -> Timestamp {
    "2026-10-12T14:00:00.250Z".parse().unwrap()
}

fn credit_transfer() -> BusinessMessage {
    BusinessMessage {
        header: AppHdr {
            from: bic("RALFECEQ"),
            to: bic("SIPCECEQ"),
            id: "M0000000000000000000000000000001".into(),
            created: at(),
            possible_duplicate: false,
        },
        document: Document::CreditTransfer(CreditTransfer {
            msg_id: "M0000000000000000000000000000001".into(),
            created: at(),
            instr_id: "BALFECEQ12102026090000000001".into(),
            end_to_end_id: "BALFECEQ12102026090000000001".into(),
            amount: "25.40".parse().unwrap(),
            instructing_agent: bic("RALFECEQ"),
            instructed_agent: bic("RBETECEQ"),
            debtor: Party {
                name: "María Ejemplo & Hijos".into(),
                id: "0000000001".into(),
                account: "1100000001".into(),
            },
            debtor_agent: bic("BALFECEQ"),
            creditor: Party { name: "Julio Tienda".into(), id: "0000000002".into(), account: "4400000001".into() },
            creditor_agent: bic("CBETECEQ"),
        }),
    }
}

fn status_report() -> BusinessMessage {
    BusinessMessage {
        header: AppHdr {
            from: bic("SIPCECEQ"),
            to: bic("RALFECEQ"),
            id: "M0000000000000000000000000000002".into(),
            created: at(),
            possible_duplicate: true,
        },
        document: Document::StatusReport(StatusReport {
            msg_id: "M0000000000000000000000000000002".into(),
            created: at(),
            original_instr_id: "BALFECEQ12102026090000000001".into(),
            original_debtor_agent: bic("BALFECEQ"),
            status: PaymentState::Rjct,
            reason: Some(ReceivedReason::Listed(ReasonCode::Am04)),
        }),
    }
}

fn golden(name: &str, message: &BusinessMessage) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
    let encoded = pretty(&encode(message)).into_bytes();
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &encoded).unwrap();
    }
    let expected =
        std::fs::read(&path).unwrap_or_else(|_| panic!("{} missing: run with UPDATE_GOLDEN=1", path.display()));
    assert_eq!(String::from_utf8(encoded).unwrap(), String::from_utf8(expected.clone()).unwrap());
    assert_eq!(&decode(&expected).unwrap(), message);
}

#[test]
fn pacs008_matches_its_golden_file() {
    golden("pacs008.xml", &credit_transfer());
}

#[test]
fn pacs002_matches_its_golden_file() {
    golden("pacs002.xml", &status_report());
}

fn reason_of(xml: &str) -> ReasonCode {
    decode(xml.as_bytes()).unwrap_err().reason
}

#[test]
fn malformed_messages_map_to_tabla_12_codes() {
    let xml = String::from_utf8(encode(&credit_transfer())).unwrap();
    assert_eq!(reason_of(&xml.replace("<EndToEndId>BALFECEQ12102026090000000001</EndToEndId>", "")), ReasonCode::Ch21);
    assert_eq!(reason_of(&xml.replace("Ccy=\"USD\"", "Ccy=\"EUR\"")), ReasonCode::Am11);
    assert_eq!(reason_of(&xml.replace(">25.40<", ">25.405<")), ReasonCode::Ch20);
    assert_eq!(reason_of(&xml.replace("</BizMsg>", "")), ReasonCode::Ff02);
    assert_eq!(reason_of(&xml.replace("pacs.008.001.08</MsgDefIdr>", "camt.056.001.08</MsgDefIdr>")), ReasonCode::Ch16);
}

proptest! {
    #[test]
    fn every_credit_transfer_round_trips(name in "\\PC{1,40}", cents in 1u64..100_000_000, account in "[0-9]{10}") {
        prop_assume!(name.trim() == name);
        let mut m = credit_transfer();
        if let Document::CreditTransfer(t) = &mut m.document {
            t.debtor.name = name;
            t.amount = Usd::from_cents(cents);
            t.creditor.account = account;
        }
        prop_assert_eq!(decode(&encode(&m)).unwrap(), m);
    }
}
