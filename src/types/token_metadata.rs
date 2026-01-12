use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TokenMetadata {
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub uri: Option<String>,
}
