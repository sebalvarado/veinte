use std::io::{self, Read, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use veinte_iso20022::{BusinessMessage, DecodeError, Document, decode, pretty};
use veinte_types::{InvalidValue, LlaveType, PaymentId, ReasonCode, ReceivedReason};

const TABLA_12: &str = "Catálogo Operacional, §5 numeral 7, Tabla 12";
const TABLA_14: &str = "Catálogo Operacional, §7, Tabla 14";
const TABLA_1: &str = "Catálogo Operacional, §3.1, Tabla 1";

#[derive(Parser)]
#[command(
    name = "veinte",
    about = "Valida mensajes, explica códigos y revisa identificadores.",
    long_about = "Valida mensajes ISO 20022, explica códigos de la Tabla 12 y revisa identificadores de pago y tipos de llave.\n\nIndependiente. No está afiliado ni respaldado por el Banco Central del Ecuador.",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Decodifica un mensaje y muestra sus campos.
    #[command(name = "validar")]
    Validate {
        /// Ruta del archivo, o - para la entrada estándar.
        #[arg(value_name = "archivo")]
        path: String,
    },
    /// Imprime el XML con sangría. No valida el mensaje; use validar.
    #[command(name = "formatear")]
    Format {
        /// Ruta del archivo, o - para la entrada estándar.
        #[arg(value_name = "archivo")]
        path: String,
    },
    /// Explica un código ISO o interno de la Tabla 12.
    #[command(name = "explicar")]
    Explain {
        /// Código ISO (AC04) o interno (0201).
        #[arg(value_name = "código")]
        code: Option<String>,
        /// Lista todos los códigos.
        #[arg(long = "todos")]
        all: bool,
    },
    /// Construye o revisa un identificador de pago.
    #[command(name = "id")]
    Id {
        #[command(subcommand)]
        command: IdCommand,
    },
    /// Lista o revisa un tipo de llave.
    #[command(name = "llave")]
    Key {
        #[command(subcommand)]
        command: KeyCommand,
    },
}

#[derive(Subcommand)]
enum IdCommand {
    /// Arma un identificador con el BIC, la fecha y hora, y el secuencial.
    #[command(name = "nuevo")]
    New {
        /// BIC de la institución ordenante.
        #[arg(long)]
        bic: String,
        /// Fecha y hora DDMMAAAAHHMMSS. No consulta el reloj.
        #[arg(long = "fecha-hora")]
        fecha_hora: String,
        /// Número secuencial o hash.
        #[arg(long)]
        secuencial: String,
    },
    /// Revisa un identificador ya armado.
    #[command(name = "revisar")]
    Check {
        /// Identificador de la orden de pago.
        #[arg(value_name = "id")]
        id: String,
    },
}

#[derive(Subcommand)]
enum KeyCommand {
    /// Lista los tipos de llave y sus códigos.
    #[command(name = "tipos")]
    Types,
    /// Revisa un código de tipo de llave.
    #[command(name = "tipo")]
    Type {
        /// Código de la Tabla 1.
        #[arg(value_name = "código")]
        code: String,
    },
}

fn main() -> ExitCode {
    match dispatch(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            let _ = writeln!(io::stderr(), "{message}");
            ExitCode::from(1)
        }
    }
}

fn dispatch(command: Command) -> Result<(), String> {
    match command {
        Command::Validate { path } => validate(&read(&path)?),
        Command::Format { path } => {
            print!("{}", pretty(&read(&path)?));
            Ok(())
        }
        Command::Explain { code, all } => explain(code, all),
        Command::Id { command } => match command {
            IdCommand::New { bic, fecha_hora, secuencial } => {
                print_id(&build_id(&bic, &format!("{bic}{fecha_hora}{secuencial}"))?);
                Ok(())
            }
            IdCommand::Check { id } => {
                let id = id.parse().map_err(valor_invalido)?;
                print_id(&id);
                Ok(())
            }
        },
        Command::Key { command } => match command {
            KeyCommand::Types => {
                for kind in LlaveType::ALL {
                    println!("{}\t{}", kind.code(), kind.description());
                }
                Ok(())
            }
            KeyCommand::Type { code } => {
                let kind: LlaveType = code.parse().map_err(valor_invalido)?;
                println!("código: {}", kind.code());
                println!("tipo: {}", kind.description());
                println!("cita: {TABLA_1}");
                Ok(())
            }
        },
    }
}

fn read(path: &str) -> Result<Vec<u8>, String> {
    if path == "-" {
        let mut buf = Vec::new();
        io::stdin().read_to_end(&mut buf).map_err(|e| format!("no se pudo leer la entrada: {e}"))?;
        return Ok(buf);
    }
    std::fs::read(path).map_err(|e| format!("no se pudo leer {path}: {e}"))
}

fn validate(bytes: &[u8]) -> Result<(), String> {
    let message = decode(bytes).map_err(rechazo)?;
    print!("{}", report(&message));
    Ok(())
}

fn rechazo(err: DecodeError) -> String {
    let code = err.reason.code();
    let description = err.reason.description();
    format!("mensaje rechazado: {code} — {description}\ndetalle: {}", detalle(&err.detail))
}

fn detalle(detail: &str) -> String {
    if let Some(path) = detail.strip_prefix("missing ") {
        return path.to_owned();
    }
    if let Some(message) = detail.strip_prefix("unsupported message ") {
        return message.trim_matches('"').to_owned();
    }
    if let Some((path, _)) = detail.split_once(": ").filter(|(path, _)| path.contains('/')) {
        return path.to_owned();
    }
    match detail {
        "empty element" => "elemento vacío",
        "unclosed element" => "elemento sin cerrar",
        "the currency is USD" => "la moneda es USD",
        "sender and receiver are the same" => "el emisor y el receptor son el mismo",
        _ => return otro_detalle(detail),
    }
    .to_owned()
}

fn otro_detalle(detail: &str) -> String {
    if let Some(amount) = detail.strip_prefix("amount ") {
        return format!("monto {}", amount.trim_matches('"'));
    }
    if let Some(status) = detail.strip_prefix("status ") {
        return format!("estado {}", status.trim_matches('"'));
    }
    if let Some(reason) = detail.strip_prefix("reason ") {
        return format!("motivo {}", reason.trim_matches('"'));
    }
    "sintaxis XML".to_owned()
}

fn valor_invalido(err: InvalidValue) -> String {
    match err.kind() {
        "PaymentId" => "identificador inválido: no cumple la Tabla 14 del Catálogo Operacional",
        "PaymentDate" => "fecha y hora inválida: DDMMAAAAHHMMSS no cumple la Tabla 14 del Catálogo Operacional",
        "Bic" => "BIC inválido: no cumple la Tabla 1 del Catálogo Operacional",
        "LlaveType" => "tipo de llave inválido: no cumple la Tabla 1 del Catálogo Operacional",
        _ => "valor inválido",
    }
    .to_owned()
}

fn report(message: &BusinessMessage) -> String {
    let mut out = String::new();
    let header = &message.header;
    line(&mut out, "tipo", message.document.definition());
    line(&mut out, "de", header.from.as_str());
    line(&mut out, "para", header.to.as_str());
    line(&mut out, "identificador", &header.id);
    line(&mut out, "creado", &header.created.to_string());
    line(&mut out, "posible duplicado", if header.possible_duplicate { "sí" } else { "no" });
    match &message.document {
        Document::CreditTransfer(transfer) => {
            line(&mut out, "identificador del mensaje", &transfer.msg_id);
            line(&mut out, "fecha de la orden", &transfer.created.to_string());
            line(&mut out, "identificador de la orden", &transfer.instr_id);
            line(&mut out, "identificador de extremo a extremo", &transfer.end_to_end_id);
            line(&mut out, "monto", &format!("{} USD", transfer.amount));
            line(&mut out, "red ordenante", transfer.instructing_agent.as_str());
            line(&mut out, "red beneficiaria", transfer.instructed_agent.as_str());
            line(&mut out, "ordenante", &transfer.debtor.name);
            line(&mut out, "identificación del ordenante", &transfer.debtor.id);
            line(&mut out, "cuenta del ordenante", &transfer.debtor.account);
            line(&mut out, "institución ordenante", transfer.debtor_agent.as_str());
            line(&mut out, "beneficiario", &transfer.creditor.name);
            line(&mut out, "identificación del beneficiario", &transfer.creditor.id);
            line(&mut out, "cuenta del beneficiario", &transfer.creditor.account);
            line(&mut out, "institución beneficiaria", transfer.creditor_agent.as_str());
        }
        Document::StatusReport(report) => {
            line(&mut out, "identificador del mensaje", &report.msg_id);
            line(&mut out, "fecha de la confirmación", &report.created.to_string());
            line(&mut out, "identificador de la orden", &report.original_instr_id);
            line(&mut out, "institución ordenante", report.original_debtor_agent.as_str());
            line(&mut out, "estado", report.status.code());
            line(&mut out, "motivo", &reason_text(report.reason.as_ref()));
        }
    }
    out
}

fn reason_text(reason: Option<&ReceivedReason>) -> String {
    match reason {
        Some(ReceivedReason::Listed(code)) => format!("{code} — {}", code.description()),
        Some(other) => other.to_string(),
        None => "—".to_owned(),
    }
}

fn line(out: &mut String, label: &str, value: &str) {
    out.push_str(label);
    out.push_str(": ");
    out.push_str(value);
    out.push('\n');
}

fn explain(code: Option<String>, all: bool) -> Result<(), String> {
    match (code, all) {
        (None, true) => {
            for code in ReasonCode::ALL {
                let internal = code.internal().unwrap_or("—");
                println!("{}\t{internal}\t{}\t{}", code.code(), code.responsible().label(), code.description());
            }
            Ok(())
        }
        (Some(code), false) => {
            let code = find_reason(&code).ok_or_else(|| format!("código desconocido: {code}"))?;
            let internal = code.internal().unwrap_or("—");
            println!("código: {}", code.code());
            println!("interno: {internal}");
            println!("descripción: {}", code.description());
            println!("responsable: {}", code.responsible().label());
            println!("cita: {TABLA_12}");
            Ok(())
        }
        _ => Err("indica un código o --todos".to_owned()),
    }
}

fn find_reason(code: &str) -> Option<ReasonCode> {
    ReasonCode::ALL.into_iter().find(|reason| reason.code() == code || reason.internal() == Some(code))
}

fn build_id(bic: &str, code: &str) -> Result<PaymentId, String> {
    let ordering = bic.parse().map_err(valor_invalido)?;
    PaymentId::new(ordering, code).map_err(valor_invalido)
}

fn print_id(id: &PaymentId) {
    println!("identificador: {}", id.as_str());
    println!("ordenante: {}", id.ordering());
    println!("cita: {TABLA_14}");
}
