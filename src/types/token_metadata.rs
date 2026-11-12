use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub uri: String,
}
