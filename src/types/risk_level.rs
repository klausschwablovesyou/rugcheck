use serde::Serialize;

/// Ordered by severity, so the overall risk is simply the maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum RiskLevel {
    Safe,
    Warning,
    Danger,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Safe => "SAFE",
            RiskLevel::Warning => "WARNING",
            RiskLevel::Danger => "DANGER",
        }
    }
}
