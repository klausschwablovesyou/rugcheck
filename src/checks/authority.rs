use anyhow::Result;
use solana_sdk::pubkey::Pubkey;
use spl_token::state::Mint;

use crate::rpc::SolanaRpc;
use crate::types::{AuthorityCheck, RiskLevel};

pub fn check_authorities(rpc: &SolanaRpc, mint_address: &Pubkey) -> Result<AuthorityCheck> {
    let mint_info = rpc.get_mint_info(mint_address)?;

    let (mint_authority, mint_authority_risk) = evaluate_mint_authority(&mint_info);
    let (freeze_authority, freeze_authority_risk) = evaluate_freeze_authority(&mint_info);

    Ok(AuthorityCheck {
        mint_authority,
        freeze_authority,
        mint_authority_risk,
        freeze_authority_risk,
    })
}

fn evaluate_mint_authority(mint: &Mint) -> (Option<String>, RiskLevel) {
    match mint.mint_authority {
        solana_sdk::program_option::COption::Some(authority) => {
            (Some(authority.to_string()), RiskLevel::Danger)
        }
        solana_sdk::program_option::COption::None => {
            (None, RiskLevel::Safe)
        }
    }
}

fn evaluate_freeze_authority(mint: &Mint) -> (Option<String>, RiskLevel) {
    match mint.freeze_authority {
        solana_sdk::program_option::COption::Some(authority) => {
            (Some(authority.to_string()), RiskLevel::Danger)
        }
        solana_sdk::program_option::COption::None => {
            (None, RiskLevel::Safe)
        }
    }
}
