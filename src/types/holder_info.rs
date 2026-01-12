use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct HolderInfo {
    pub address: String,
    pub balance: u64,
    pub percentage: f64,
}
