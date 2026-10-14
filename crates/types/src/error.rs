use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidValue {
    kind: &'static str,
    rule: &'static str,
}

impl InvalidValue {
    pub const fn new(kind: &'static str, rule: &'static str) -> Self {
        Self { kind, rule }
    }

    pub fn kind(&self) -> &'static str {
        self.kind
    }
}

impl fmt::Display for InvalidValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {}: {}", self.kind, self.rule)
    }
}

impl std::error::Error for InvalidValue {}
