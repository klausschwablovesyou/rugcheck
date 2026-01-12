use anyhow::Result;
use solana_sdk::pubkey::Pubkey;

use crate::rpc::SolanaRpc;
use crate::types::{HolderInfo, HoldersCheck, RiskLevel};

pub fn check_holders(rpc: &SolanaRpc, mint_address: &Pubkey, total_supply: u64) -> Result<HoldersCheck> {
    let largest_accounts = rpc.get_largest_token_accounts(mint_address)?;

    let top_holders: Vec<HolderInfo> = largest_accounts
        .into_iter()
        .take(10)
        .map(|(address, balance)| {
            let percentage = if total_supply > 0 {
                (balance as f64 / total_supply as f64) * 100.0
            } else {
                0.0
            };
            HolderInfo {
                address: address.to_string(),
                balance,
                percentage,
            }
        })
        .collect();

    let top_10_concentration: f64 = top_holders.iter().map(|h| h.percentage).sum();

    let risk = evaluate_concentration(top_10_concentration);

    Ok(HoldersCheck {
        top_holders,
        top_10_concentration,
        risk,
    })
}

fn evaluate_concentration(concentration: f64) -> RiskLevel {
    if concentration >= 80.0 {
        RiskLevel::Danger
    } else if concentration >= 50.0 {
        RiskLevel::Warning
    } else {
        RiskLevel::Safe
    }
}
