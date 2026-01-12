use serde::Serialize;
use super::holder_info::HolderInfo;
use super::risk_level::RiskLevel;

#[derive(Debug, Clone, Serialize)]
pub struct HoldersCheck {
    pub top_holders: Vec<HolderInfo>,
    pub top_10_concentration: f64,
    pub risk: RiskLevel,
}
