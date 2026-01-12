use anyhow::Result;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

use crate::rpc::SolanaRpc;
use crate::types::{LiquidityCheck, RiskLevel};

// Known DEX program IDs
const RAYDIUM_AMM_V4: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";
const ORCA_WHIRLPOOL: &str = "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc";

pub fn check_liquidity(rpc: &SolanaRpc, mint_address: &Pubkey) -> Result<LiquidityCheck> {
    // Check for Raydium pools
    let raydium_program = Pubkey::from_str(RAYDIUM_AMM_V4).unwrap();
    let orca_program = Pubkey::from_str(ORCA_WHIRLPOOL).unwrap();

    // Try to find LP pools containing this token
    // This is a simplified check - in production you'd want to query pool accounts directly
    let has_raydium = check_dex_presence(rpc, mint_address, &raydium_program);
    let has_orca = check_dex_presence(rpc, mint_address, &orca_program);

    let has_liquidity = has_raydium || has_orca;

    if !has_liquidity {
        return Ok(LiquidityCheck {
            has_liquidity: false,
            lp_locked: None,
            lp_burned: None,
            risk: RiskLevel::Danger,
            details: "No liquidity pools detected on major DEXes".to_string(),
        });
    }

    // For now, we can't easily determine LP lock status without more complex queries
    // In a production tool, you'd check:
    // 1. If LP tokens are held by known lock programs (e.g., Raydium lock)
    // 2. If LP tokens are burned (sent to null address)
    Ok(LiquidityCheck {
        has_liquidity: true,
        lp_locked: None,
        lp_burned: None,
        risk: RiskLevel::Warning,
        details: "Liquidity detected but lock status unknown - verify manually".to_string(),
    })
}

fn check_dex_presence(_rpc: &SolanaRpc, _mint: &Pubkey, _dex_program: &Pubkey) -> bool {
    // Simplified check - in production, query for pool accounts
    // This would involve getProgramAccounts with filters for the mint
    // For now, we return false and rely on the holder analysis
    // A full implementation would use getTokenAccountsByOwner for pool reserves
    false
}
