use solana_sdk::{program_option::COption, pubkey::Pubkey};
use spl_token_2022::state::Mint;

use crate::types::{AuthorityCheck, RiskLevel};

pub fn check_authorities(mint: &Mint) -> AuthorityCheck {
    let (mint_authority, mint_authority_risk) = evaluate_authority(mint.mint_authority);
    let (freeze_authority, freeze_authority_risk) = evaluate_authority(mint.freeze_authority);

    AuthorityCheck {
        mint_authority,
        freeze_authority,
        mint_authority_risk,
        freeze_authority_risk,
    }
}

fn evaluate_authority(authority: COption<Pubkey>) -> (Option<String>, RiskLevel) {
    match authority {
        COption::Some(authority) => (Some(authority.to_string()), RiskLevel::Danger),
        COption::None => (None, RiskLevel::Safe),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revoked_authorities_are_safe() {
        let mint = Mint {
            is_initialized: true,
            ..Mint::default()
        };
        let check = check_authorities(&mint);
        assert_eq!(check.mint_authority, None);
        assert_eq!(check.freeze_authority, None);
        assert_eq!(check.mint_authority_risk, RiskLevel::Safe);
        assert_eq!(check.freeze_authority_risk, RiskLevel::Safe);
    }

    #[test]
    fn active_mint_authority_is_danger() {
        let authority = Pubkey::new_unique();
        let mint = Mint {
            is_initialized: true,
            mint_authority: COption::Some(authority),
            ..Mint::default()
        };
        let check = check_authorities(&mint);
        assert_eq!(check.mint_authority, Some(authority.to_string()));
        assert_eq!(check.mint_authority_risk, RiskLevel::Danger);
        assert_eq!(check.freeze_authority, None);
        assert_eq!(check.freeze_authority_risk, RiskLevel::Safe);
    }

    #[test]
    fn active_authorities_are_reported_independently() {
        let freeze = Pubkey::new_unique();
        let mint = Mint {
            is_initialized: true,
            freeze_authority: COption::Some(freeze),
            ..Mint::default()
        };
        let check = check_authorities(&mint);
        assert_eq!(check.mint_authority_risk, RiskLevel::Safe);
        assert_eq!(check.freeze_authority, Some(freeze.to_string()));
        assert_eq!(check.freeze_authority_risk, RiskLevel::Danger);
    }
}
