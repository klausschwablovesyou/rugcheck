use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SupplyInfo {
    pub total_supply: u64,
    pub decimals: u8,
    pub formatted_supply: String,
}
