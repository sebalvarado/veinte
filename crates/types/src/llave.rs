use std::fmt;
use std::str::FromStr;

use crate::InvalidValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LlaveType {
    Celular,
    Identificacion,
    Correo,
    Alfanumerico,
}

impl LlaveType {
    pub const ALL: [Self; 4] = [Self::Celular, Self::Identificacion, Self::Correo, Self::Alfanumerico];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Celular => "0001",
            Self::Identificacion => "0002",
            Self::Correo => "0003",
            Self::Alfanumerico => "0004",
        }
    }

    /// Catálogo Operacional §3.1, Tabla 1, column TIPO DE LLAVE.
    ///
    /// Tabla 1 also gives a field format for each type. Those formats are not represented here.
    pub const fn description(self) -> &'static str {
        match self {
            Self::Celular => "número del celular",
            Self::Identificacion => "número de identificación",
            Self::Correo => "correo electrónico",
            Self::Alfanumerico => "alfanumérico",
        }
    }
}

impl FromStr for LlaveType {
    type Err = InvalidValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL.into_iter().find(|t| t.code() == s).ok_or(InvalidValue::new("LlaveType", "a listed llave type code"))
    }
}

impl fmt::Display for LlaveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_tabla_1() {
        let codes: Vec<_> = LlaveType::ALL.iter().map(|t| t.code()).collect();
        assert_eq!(codes, ["0001", "0002", "0003", "0004"]);
        for t in LlaveType::ALL {
            assert_eq!(t.code().parse(), Ok(t));
        }
        assert!("0005".parse::<LlaveType>().is_err());
    }

    #[test]
    fn celular_is_tabla_1() {
        assert_eq!(LlaveType::Celular.description(), "número del celular");
    }

    #[test]
    fn identificacion_is_tabla_1() {
        assert_eq!(LlaveType::Identificacion.description(), "número de identificación");
    }

    #[test]
    fn correo_is_tabla_1() {
        assert_eq!(LlaveType::Correo.description(), "correo electrónico");
    }

    #[test]
    fn alfanumerico_is_tabla_1() {
        assert_eq!(LlaveType::Alfanumerico.description(), "alfanumérico");
    }
}
