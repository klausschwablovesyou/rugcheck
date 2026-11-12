use serde::Serialize;
use super::risk_level::RiskLevel;

/// Token-2022 extensions present on a mint and the risks they carry.
#[derive(Debug, Clone, Serialize)]
pub struct ExtensionsCheck {
    pub extensions: Vec<String>,
    pub risk: RiskLevel,
    pub risk_factors: Vec<String>,
}
