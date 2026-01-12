use serde::Serialize;
use super::risk_level::RiskLevel;

#[derive(Debug, Clone, Serialize)]
pub struct LiquidityCheck {
    pub has_liquidity: bool,
    pub lp_locked: Option<bool>,
    pub lp_burned: Option<bool>,
    pub risk: RiskLevel,
    pub details: String,
}
