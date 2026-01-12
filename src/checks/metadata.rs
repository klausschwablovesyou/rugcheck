use anyhow::Result;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

use crate::rpc::SolanaRpc;
use crate::types::TokenMetadata;

const METADATA_PROGRAM_ID: &str = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";

pub fn fetch_metadata(rpc: &SolanaRpc, mint_address: &Pubkey) -> Result<Option<TokenMetadata>> {
    let metadata_program = Pubkey::from_str(METADATA_PROGRAM_ID).unwrap();

    // Derive the metadata PDA
    let seeds = &[
        b"metadata",
        metadata_program.as_ref(),
        mint_address.as_ref(),
    ];
    let (metadata_pda, _) = Pubkey::find_program_address(seeds, &metadata_program);

    // Try to fetch the metadata account
    let data = match rpc.get_account_data(&metadata_pda) {
        Ok(data) => data,
        Err(_) => return Ok(None),
    };

    // Parse metadata (simplified - full parsing would use mpl-token-metadata)
    let metadata = parse_metadata_data(&data)?;

    Ok(Some(metadata))
}

fn parse_metadata_data(data: &[u8]) -> Result<TokenMetadata> {
    // Metadata account layout (simplified):
    // - 1 byte: key
    // - 32 bytes: update authority
    // - 32 bytes: mint
    // - 4 bytes: name length + name (max 32)
    // - 4 bytes: symbol length + symbol (max 10)
    // - 4 bytes: uri length + uri (max 200)

    if data.len() < 69 {
        return Ok(TokenMetadata {
            name: None,
            symbol: None,
            uri: None,
        });
    }

    let mut offset = 1 + 32 + 32; // Skip key, update authority, mint

    let name = read_string(data, &mut offset)?;
    let symbol = read_string(data, &mut offset)?;
    let uri = read_string(data, &mut offset)?;

    Ok(TokenMetadata {
        name: Some(name.trim_matches('\0').to_string()),
        symbol: Some(symbol.trim_matches('\0').to_string()),
        uri: Some(uri.trim_matches('\0').to_string()),
    })
}

fn read_string(data: &[u8], offset: &mut usize) -> Result<String> {
    if *offset + 4 > data.len() {
        return Ok(String::new());
    }

    let len = u32::from_le_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ]) as usize;

    *offset += 4;

    if *offset + len > data.len() {
        return Ok(String::new());
    }

    let s = String::from_utf8_lossy(&data[*offset..*offset + len]).to_string();
    *offset += len;

    Ok(s)
}
