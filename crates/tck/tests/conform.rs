use std::io::Read;

use jiff::Timestamp;
use veinte_iso20022::Signer;
use veinte_tck::{Parties, examine, passed, render};
use veinte_types::Bic;

fn key() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom").unwrap().read_exact(&mut bytes).unwrap();
    bytes
}

fn signer(bic: &str) -> Signer {
    Signer::from_bytes(bic.parse::<Bic>().unwrap(), key())
}

#[test]
fn the_reference_connector_passes_every_scenario() {
    let now: Timestamp = "2026-10-12T14:00:00.250Z".parse().unwrap();
    let ordering = signer("BALFECEQ");
    let ordering_red = signer("RALFECEQ");
    let beneficiary = signer("CBETECEQ");
    let beneficiary_red = signer("RBETECEQ");
    let findings = examine(Parties {
        ordering: &ordering,
        ordering_red: &ordering_red,
        beneficiary: &beneficiary,
        beneficiary_red: &beneficiary_red,
        now,
    });
    let report = render(&findings);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/informe.txt");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &report).unwrap();
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{} missing", path.display()));
    assert!(passed(&findings), "{report}");
    assert_eq!(report, expected);
}
