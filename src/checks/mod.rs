mod authority;
mod holders;
mod liquidity;
mod metadata;
mod supply;

pub use authority::check_authorities;
pub use holders::check_holders;
pub use liquidity::check_liquidity;
pub use metadata::fetch_metadata;
pub use supply::fetch_supply;

use anyhow::Result;
use solana_sdk::pubkey::Pubkey;

use crate::rpc::SolanaRpc;
use crate::types::RugCheckResult;

pub fn run_all_checks(rpc: &SolanaRpc, mint_address: &Pubkey) -> Result<RugCheckResult> {
    let mut result = RugCheckResult::new(*mint_address);

    // Fetch supply first (needed for holder calculations)
    match fetch_supply(rpc, mint_address) {
        Ok(supply) => result.supply = Some(supply),
        Err(e) => eprintln!("Warning: Failed to fetch supply: {}", e),
    }

    // Fetch metadata
    match fetch_metadata(rpc, mint_address) {
        Ok(metadata) => result.metadata = metadata,
        Err(e) => eprintln!("Warning: Failed to fetch metadata: {}", e),
    }

    // Check authorities
    match check_authorities(rpc, mint_address) {
        Ok(auth) => result.authority = Some(auth),
        Err(e) => eprintln!("Warning: Failed to check authorities: {}", e),
    }

    // Check holders
    let total_supply = result.supply.as_ref().map(|s| s.total_supply).unwrap_or(0);
    match check_holders(rpc, mint_address, total_supply) {
        Ok(holders) => result.holders = Some(holders),
        Err(e) => eprintln!("Warning: Failed to check holders: {}", e),
    }

    // Check liquidity
    match check_liquidity(rpc, mint_address) {
        Ok(liq) => result.liquidity = Some(liq),
        Err(e) => eprintln!("Warning: Failed to check liquidity: {}", e),
    }

    // Calculate overall risk
    result.calculate_overall_risk();

    Ok(result)
}
