use anyhow::Result;
use solana_sdk::pubkey::Pubkey;

use crate::rpc::SolanaRpc;
use crate::types::SupplyInfo;

pub fn fetch_supply(rpc: &SolanaRpc, mint_address: &Pubkey) -> Result<SupplyInfo> {
    let mint_info = rpc.get_mint_info(mint_address)?;

    let total_supply = mint_info.supply;
    let decimals = mint_info.decimals;

    let formatted_supply = format_supply(total_supply, decimals);

    Ok(SupplyInfo {
        total_supply,
        decimals,
        formatted_supply,
    })
}

fn format_supply(supply: u64, decimals: u8) -> String {
    let divisor = 10u64.pow(decimals as u32);
    let whole = supply / divisor;
    let frac = supply % divisor;

    if decimals == 0 {
        format_with_commas(whole)
    } else {
        let frac_str = format!("{:0>width$}", frac, width = decimals as usize);
        let trimmed = frac_str.trim_end_matches('0');
        if trimmed.is_empty() {
            format_with_commas(whole)
        } else {
            format!("{}.{}", format_with_commas(whole), trimmed)
        }
    }
}

fn format_with_commas(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
