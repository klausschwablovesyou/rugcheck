use serde::Serialize;
use super::risk_level::RiskLevel;

#[derive(Debug, Clone, Serialize)]
pub struct AuthorityCheck {
    pub mint_authority: Option<String>,
    pub freeze_authority: Option<String>,
    pub mint_authority_risk: RiskLevel,
    pub freeze_authority_risk: RiskLevel,
}
