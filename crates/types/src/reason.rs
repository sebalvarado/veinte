use std::fmt;
use std::str::FromStr;

use crate::InvalidValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Responsible {
    Beneficiary,
    Sip,
    Any,
}

impl Responsible {
    /// Catálogo Operacional §5 numeral 7, Tabla 12, column RESPONSABLE.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Beneficiary => "IR Beneficiario",
            Self::Sip => "SIP",
            Self::Any => "Todos",
        }
    }
}

macro_rules! reason_codes {
    ($($variant:ident $iso:literal $internal:expr, $party:ident, $description:literal;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum ReasonCode {
            $($variant,)*
        }

        impl ReasonCode {
            pub const ALL: [Self; 21] = [$(Self::$variant),*];

            pub const fn code(self) -> &'static str {
                match self { $(Self::$variant => $iso,)* }
            }

            pub const fn internal(self) -> Option<&'static str> {
                match self { $(Self::$variant => $internal,)* }
            }

            pub const fn responsible(self) -> Responsible {
                match self { $(Self::$variant => Responsible::$party,)* }
            }

            /// Catálogo Operacional §5 numeral 7, Tabla 12, column DESCRIPCIÓN.
            ///
            /// Where the table pairs an internal code with the text, the description keeps both,
            /// as in `0201 (PGO RECHAZADO, CUENTA CERRADA)`. Wording, including missing accents,
            /// follows the published table.
            pub const fn description(self) -> &'static str {
                match self { $(Self::$variant => $description,)* }
            }
        }
    };
}

reason_codes! {
    Ac04 "AC04" Some("0201"), Beneficiary, "0201 (PGO RECHAZADO, CUENTA CERRADA)";
    Ac01 "AC01" Some("0202"), Beneficiary, "0202 (PGO RECHAZADO, CUENTA NO EXISTE)";
    Ac06 "AC06" Some("0203"), Beneficiary, "0203 (PGO RECHAZADO, CUENTA BLOQUEADA)";
    Ac05 "AC05" Some("0204"), Beneficiary, "0204 (PGO RECHAZADO, ORDEN BENEFICIARIO)";
    Dt01 "DT01" Some("0205"), Any, "0205 (PGO RECHAZADO, POR NO CONFIRMACIÓN TIME OUT)";
    Ac07 "AC07" Some("0206"), Beneficiary, "0206 (PGO RECHAZADO, CUENTA INACTIVA)";
    Am04 "AM04" Some("0207"), Sip, "0207 (INSUFICIENCIA DE FONDOS)";
    Ms01 "MS01" Some("0208"), Sip, "0208 (INFORMACIÓN INCORRECTA)";
    Am11 "AM11" None, Sip, "Tipo de moneda no valida";
    Ch16 "CH16" None, Sip, "Contenido incorrecto";
    Ch21 "CH21" None, Sip, "Elemento obligatorio ausente.";
    Dt02 "DT02" None, Sip, "Fecha no válida";
    Ds0k "DS0K" None, Sip, "Número de transacciones no permitido";
    Ch20 "CH20" None, Sip, "Formato de monto incorrecto";
    Am01 "AM01" None, Sip, "Monto cero";
    Du01 "DU01" None, Sip, "La identificación del mensaje no es única";
    Am02 "AM02" None, Sip, "Monto mayor al permitido";
    Ac02 "AC02" None, Sip, "Número de cuenta inválido";
    Ta01 "TA01" None, Sip, "Error de comunicación";
    Ff10 "FF10" None, Sip, "Problemas técnicos";
    Ff02 "FF02" None, Sip, "Sintaxis de mensaje incorrecto";
}

impl FromStr for ReasonCode {
    type Err = InvalidValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL.into_iter().find(|r| r.code() == s).ok_or(InvalidValue::new("ReasonCode", "a listed reason code"))
    }
}

impl fmt::Display for ReasonCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReceivedReason {
    Listed(ReasonCode),
    Unlisted([u8; 4]),
}

impl ReceivedReason {
    pub fn code(&self) -> &str {
        match self {
            Self::Listed(r) => r.code(),
            Self::Unlisted(b) => std::str::from_utf8(b).expect("validated as ASCII"),
        }
    }
}

impl FromStr for ReceivedReason {
    type Err = InvalidValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(listed) = s.parse() {
            return Ok(Self::Listed(listed));
        }
        <[u8; 4]>::try_from(s.as_bytes())
            .ok()
            .filter(|b| b.iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
            .map(Self::Unlisted)
            .ok_or(InvalidValue::new("ReceivedReason", "four uppercase ASCII letters or digits"))
    }
}

impl fmt::Display for ReceivedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_tabla_12() {
        let beneficiary: Vec<_> = ReasonCode::ALL
            .into_iter()
            .filter(|r| r.responsible() == Responsible::Beneficiary)
            .map(ReasonCode::code)
            .collect();
        assert_eq!(beneficiary, ["AC04", "AC01", "AC06", "AC05", "AC07"]);
        let internal: Vec<_> = ReasonCode::ALL.into_iter().filter_map(ReasonCode::internal).collect();
        assert_eq!(internal, ["0201", "0202", "0203", "0204", "0205", "0206", "0207", "0208"]);
        assert_eq!(ReasonCode::Dt01.responsible(), Responsible::Any);
        assert_eq!(ReasonCode::Am04.responsible(), Responsible::Sip);
    }

    #[test]
    fn every_code_round_trips() {
        for r in ReasonCode::ALL {
            assert_eq!(r.code().parse(), Ok(r));
            assert_eq!(r.code().parse(), Ok(ReceivedReason::Listed(r)));
        }
    }

    #[test]
    fn tabla_12_names_the_responsible_party() {
        assert_eq!(Responsible::Beneficiary.label(), "IR Beneficiario");
        assert_eq!(Responsible::Sip.label(), "SIP");
        assert_eq!(Responsible::Any.label(), "Todos");
    }

    #[test]
    fn ac04_is_cuenta_cerrada() {
        assert_eq!(ReasonCode::Ac04.description(), "0201 (PGO RECHAZADO, CUENTA CERRADA)");
    }

    #[test]
    fn ac01_is_cuenta_inexistente() {
        assert_eq!(ReasonCode::Ac01.description(), "0202 (PGO RECHAZADO, CUENTA NO EXISTE)");
    }

    #[test]
    fn ac06_is_cuenta_bloqueada() {
        assert_eq!(ReasonCode::Ac06.description(), "0203 (PGO RECHAZADO, CUENTA BLOQUEADA)");
    }

    #[test]
    fn ac05_is_orden_beneficiario() {
        assert_eq!(ReasonCode::Ac05.description(), "0204 (PGO RECHAZADO, ORDEN BENEFICIARIO)");
    }

    #[test]
    fn dt01_is_timeout() {
        assert_eq!(ReasonCode::Dt01.description(), "0205 (PGO RECHAZADO, POR NO CONFIRMACIÓN TIME OUT)");
    }

    #[test]
    fn ac07_is_cuenta_inactiva() {
        assert_eq!(ReasonCode::Ac07.description(), "0206 (PGO RECHAZADO, CUENTA INACTIVA)");
    }

    #[test]
    fn am04_is_insuficiencia_de_fondos() {
        assert_eq!(ReasonCode::Am04.description(), "0207 (INSUFICIENCIA DE FONDOS)");
    }

    #[test]
    fn ms01_is_informacion_incorrecta() {
        assert_eq!(ReasonCode::Ms01.description(), "0208 (INFORMACIÓN INCORRECTA)");
    }

    #[test]
    fn am11_is_moneda_no_valida() {
        assert_eq!(ReasonCode::Am11.description(), "Tipo de moneda no valida");
    }

    #[test]
    fn ch16_is_contenido_incorrecto() {
        assert_eq!(ReasonCode::Ch16.description(), "Contenido incorrecto");
    }

    #[test]
    fn ch21_is_elemento_ausente() {
        assert_eq!(ReasonCode::Ch21.description(), "Elemento obligatorio ausente.");
    }

    #[test]
    fn dt02_is_fecha_no_valida() {
        assert_eq!(ReasonCode::Dt02.description(), "Fecha no válida");
    }

    #[test]
    fn ds0k_is_numero_de_transacciones() {
        assert_eq!(ReasonCode::Ds0k.description(), "Número de transacciones no permitido");
    }

    #[test]
    fn ch20_is_formato_de_monto() {
        assert_eq!(ReasonCode::Ch20.description(), "Formato de monto incorrecto");
    }

    #[test]
    fn am01_is_monto_cero() {
        assert_eq!(ReasonCode::Am01.description(), "Monto cero");
    }

    #[test]
    fn du01_is_identificacion_no_unica() {
        assert_eq!(ReasonCode::Du01.description(), "La identificación del mensaje no es única");
    }

    #[test]
    fn am02_is_monto_mayor_al_permitido() {
        assert_eq!(ReasonCode::Am02.description(), "Monto mayor al permitido");
    }

    #[test]
    fn ac02_is_cuenta_invalida() {
        assert_eq!(ReasonCode::Ac02.description(), "Número de cuenta inválido");
    }

    #[test]
    fn ta01_is_error_de_comunicacion() {
        assert_eq!(ReasonCode::Ta01.description(), "Error de comunicación");
    }

    #[test]
    fn ff10_is_problemas_tecnicos() {
        assert_eq!(ReasonCode::Ff10.description(), "Problemas técnicos");
    }

    #[test]
    fn ff02_is_sintaxis_incorrecta() {
        assert_eq!(ReasonCode::Ff02.description(), "Sintaxis de mensaje incorrecto");
    }

    #[test]
    fn keeps_unlisted_codes_as_evidence() {
        let received: ReceivedReason = "NARR".parse().unwrap();
        assert_eq!(received, ReceivedReason::Unlisted(*b"NARR"));
        assert_eq!(received.to_string(), "NARR");
        assert!("NARR".parse::<ReasonCode>().is_err());
        for s in ["narr", "NAR", "NARRA", "NA-R"] {
            assert!(s.parse::<ReceivedReason>().is_err(), "{s:?} parsed");
        }
    }
}
