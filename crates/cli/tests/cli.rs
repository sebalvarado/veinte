use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_veinte"))
}

fn run(args: &[impl AsRef<std::ffi::OsStr>]) -> Output {
    bin().args(args).output().expect("run veinte")
}

fn run_stdin(args: &[&str], stdin: &[u8]) -> Output {
    let mut child =
        bin().args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

fn golden(name: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name)).unwrap()
}

fn iso(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../iso20022/tests/golden").join(name)
}

fn assert_ok(out: &Output, name: &str) {
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), golden(name));
    assert!(out.stderr.is_empty(), "{}", String::from_utf8_lossy(&out.stderr));
}

fn assert_err(out: &Output, name: &str) {
    assert_eq!(out.status.code(), Some(1), "stdout: {}", String::from_utf8_lossy(&out.stdout));
    assert!(out.stdout.is_empty(), "{}", String::from_utf8_lossy(&out.stdout));
    assert_eq!(String::from_utf8_lossy(&out.stderr), golden(name));
}

#[test]
fn validar_prints_the_pacs008_fields() {
    assert_ok(&run(&["validar", iso("pacs008.xml").to_str().unwrap()]), "validar-pacs008.txt");
}

#[test]
fn validar_prints_the_pacs002_fields() {
    let path = iso("pacs002.xml");
    assert_ok(&run(&["validar", path.to_str().unwrap()]), "validar-pacs002.txt");
}

#[test]
fn validar_reads_stdin() {
    let bytes = fs::read(iso("pacs008.xml")).unwrap();
    assert_ok(&run_stdin(&["validar", "-"], &bytes), "validar-pacs008.txt");
}

#[test]
fn validar_exits_when_a_required_element_is_missing() {
    let xml = fs::read_to_string(iso("pacs008.xml"))
        .unwrap()
        .replace("<EndToEndId>BALFECEQ12102026090000000001</EndToEndId>", "");
    let path = std::env::temp_dir().join(format!("veinte-missing-e2e-{}.xml", std::process::id()));
    fs::write(&path, xml).unwrap();
    assert_err(&run(&["validar", path.to_str().unwrap()]), "validar-elemento-ausente.txt");
    let _ = fs::remove_file(path);
}

#[test]
fn validar_reports_an_unsupported_acmt_message() {
    let xml = fs::read_to_string(iso("pacs008.xml"))
        .unwrap()
        .replace("<MsgDefIdr>pacs.008.001.08</MsgDefIdr>", "<MsgDefIdr>acmt.998.001.bce01.in</MsgDefIdr>");
    let path = std::env::temp_dir().join(format!("veinte-acmt-{}.xml", std::process::id()));
    fs::write(&path, xml).unwrap();
    assert_err(&run(&["validar", path.to_str().unwrap()]), "validar-acmt.txt");
    let _ = fs::remove_file(path);
}

#[test]
fn formatear_pretty_prints_the_golden_messages() {
    for (input, expected) in [("pacs008.xml", "formatear-pacs008.xml"), ("pacs002.xml", "formatear-pacs002.xml")] {
        let out = run(&["formatear", iso(input).to_str().unwrap()]);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(expected);
        assert_eq!(out.stdout, fs::read(path).unwrap());
        assert!(out.stderr.is_empty());
    }
}

#[test]
fn formatear_reads_stdin() {
    let bytes = fs::read(iso("pacs002.xml")).unwrap();
    let out = run_stdin(&["formatear", "-"], &bytes);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/formatear-pacs002.xml");
    assert_eq!(out.stdout, fs::read(expected).unwrap());
}

#[test]
fn explicar_prints_an_iso_code() {
    assert_ok(&run(&["explicar", "AC04"]), "explicar-ac04.txt");
}

#[test]
fn explicar_accepts_the_internal_code() {
    assert_ok(&run(&["explicar", "0201"]), "explicar-ac04.txt");
}

#[test]
fn explicar_prints_a_code_without_an_internal_number() {
    assert_ok(&run(&["explicar", "AM11"]), "explicar-am11.txt");
}

#[test]
fn explicar_lists_every_code() {
    assert_ok(&run(&["explicar", "--todos"]), "explicar-todos.txt");
}

#[test]
fn explicar_rejects_an_unknown_code() {
    for code in ["NARR", "ac04", "201"] {
        let out = run(&["explicar", code]);
        assert_eq!(out.status.code(), Some(1), "{code}");
        assert_eq!(String::from_utf8_lossy(&out.stderr), format!("código desconocido: {code}\n"));
    }
}

#[test]
fn explicar_requires_a_code_or_todos() {
    for args in [vec!["explicar"], vec!["explicar", "--todos", "AC04"]] {
        let out = run(&args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert_eq!(String::from_utf8_lossy(&out.stderr), "indica un código o --todos\n");
    }
}

#[test]
fn id_nuevo_builds_a_payment_id() {
    assert_ok(
        &run(&["id", "nuevo", "--bic", "BALFECEQ", "--fecha-hora", "10102026093000", "--secuencial", "A1B2C3D4E5F6G"]),
        "id.txt",
    );
}

#[test]
fn id_revisar_checks_a_payment_id() {
    assert_ok(&run(&["id", "revisar", "BALFECEQ10102026093000A1B2C3D4E5F6G"]), "id.txt");
}

#[test]
fn id_nuevo_accepts_an_explicit_bic_when_the_string_is_ambiguous() {
    assert_ok(
        &run(&["id", "nuevo", "--bic", "AB", "--fecha-hora", "10102026093000", "--secuencial", "12XYZ"]),
        "id-bic-explicito.txt",
    );
}

#[test]
fn id_revisar_rejects_an_ambiguous_id() {
    assert_err(&run(&["id", "revisar", "AB1010202609300012XYZ"]), "id-revisar-invalido.txt");
}

#[test]
fn id_nuevo_rejects_a_bad_date() {
    assert_err(
        &run(&["id", "nuevo", "--bic", "BALFECEQ", "--fecha-hora", "32102026093000", "--secuencial", "A1"]),
        "id-fecha-invalida.txt",
    );
}

#[test]
fn id_nuevo_rejects_a_malformed_bic() {
    let out = run(&["id", "nuevo", "--bic", "no", "--fecha-hora", "10102026093000", "--secuencial", "ABCD"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert_eq!(String::from_utf8_lossy(&out.stderr), "BIC inválido: no cumple la Tabla 1 del Catálogo Operacional\n");
}

#[test]
fn llave_tipos_lists_tabla_1() {
    assert_ok(&run(&["llave", "tipos"]), "llave-tipos.txt");
}

#[test]
fn llave_tipo_checks_a_code() {
    assert_ok(&run(&["llave", "tipo", "0002"]), "llave-tipo-0002.txt");
}

#[test]
fn llave_tipo_rejects_an_unknown_code() {
    let out = run(&["llave", "tipo", "0005"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "tipo de llave inválido: no cumple la Tabla 1 del Catálogo Operacional\n"
    );
}

#[test]
fn formatear_help_says_it_does_not_validate() {
    let out = run(&["formatear", "--help"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("No valida"), "{stdout}");
    assert!(stdout.contains("validar"), "{stdout}");
}

#[test]
fn help_keeps_the_non_affiliation_disclaimer() {
    let out = run(&["--help"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Banco Central del Ecuador"), "{stdout}");
}
