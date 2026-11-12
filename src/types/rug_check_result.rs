use serde::Serialize;
use solana_sdk::pubkey::Pubkey;
use super::authority_check::AuthorityCheck;
use super::extensions_check::ExtensionsCheck;
use super::holders_check::HoldersCheck;
use super::risk_level::RiskLevel;
use super::supply_info::SupplyInfo;
use super::token_metadata::TokenMetadata;

#[derive(Debug, Clone, Serialize)]
pub struct RugCheckResult {
    pub token_address: String,
    pub token_program: String,
    pub metadata: Option<TokenMetadata>,
    pub supply: SupplyInfo,
    pub authority: AuthorityCheck,
    /// `None` when the mint has no Token-2022 extensions.
    pub extensions: Option<ExtensionsCheck>,
    /// `None` when the holder data could not be fetched.
    pub holders: Option<HoldersCheck>,
    pub overall_risk: RiskLevel,
    pub risk_factors: Vec<String>,
    /// What the verdict does not cover: always "liquidity", plus "holders"
    /// when the holder data was unavailable.
    pub not_checked: Vec<String>,
}

impl RugCheckResult {
    pub fn new(
        token_address: &Pubkey,
        token_program: &str,
        metadata: Option<TokenMetadata>,
        supply: SupplyInfo,
        authority: AuthorityCheck,
        extensions: Option<ExtensionsCheck>,
        holders: Option<HoldersCheck>,
    ) -> Self {
        let (overall_risk, risk_factors) =
            assess_risk(&supply, &authority, extensions.as_ref(), holders.as_ref());
        let mut not_checked = vec!["liquidity".to_string()];
        if holders.is_none() {
            not_checked.push("holders".to_string());
        }

        Self {
            token_address: token_address.to_string(),
            token_program: token_program.to_string(),
            metadata,
            supply,
            authority,
            extensions,
            holders,
            overall_risk,
            risk_factors,
            not_checked,
        }
    }
}

/// Combines the individual checks: the overall risk is the highest level
/// found, and every finding above `Safe` is listed as a risk factor.
fn assess_risk(
    supply: &SupplyInfo,
    authority: &AuthorityCheck,
    extensions: Option<&ExtensionsCheck>,
    holders: Option<&HoldersCheck>,
) -> (RiskLevel, Vec<String>) {
    let mut overall = RiskLevel::Safe;
    let mut factors = Vec::new();
    let mut flag = |level: RiskLevel, factor: String| {
        if level > RiskLevel::Safe {
            overall = overall.max(level);
            factors.push(factor);
        }
    };

    flag(
        authority.mint_authority_risk,
        "Mint authority is active - unlimited tokens can be minted".to_string(),
    );
    flag(
        authority.freeze_authority_risk,
        "Freeze authority is active - accounts can be frozen".to_string(),
    );

    if let Some(extensions) = extensions {
        for factor in &extensions.risk_factors {
            flag(extensions.risk, factor.clone());
        }
    }

    if supply.total_supply == 0 {
        flag(RiskLevel::Warning, "Total supply is zero".to_string());
    }

    match holders {
        Some(holders) => {
            let label = if holders.risk == RiskLevel::Danger { "High" } else { "Moderate" };
            flag(
                holders.risk,
                format!(
                    "{label} concentration: top 10 token accounts hold {:.1}% of the supply",
                    holders.top_10_concentration
                ),
            );
        }
        // An unchecked item must never read as a clean bill of health.
        None => flag(
            RiskLevel::Warning,
            "Holder concentration could not be checked".to_string(),
        ),
    }

    (overall, factors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supply(total_supply: u64) -> SupplyInfo {
        SupplyInfo {
            total_supply,
            decimals: 0,
            formatted_supply: total_supply.to_string(),
        }
    }

    fn authority(mint: RiskLevel, freeze: RiskLevel) -> AuthorityCheck {
        AuthorityCheck {
            mint_authority: None,
            freeze_authority: None,
            mint_authority_risk: mint,
            freeze_authority_risk: freeze,
        }
    }

    fn holders(concentration: f64, risk: RiskLevel) -> HoldersCheck {
        HoldersCheck {
            top_holders: Vec::new(),
            top_10_concentration: concentration,
            risk,
        }
    }

    const SAFE: RiskLevel = RiskLevel::Safe;
    const WARNING: RiskLevel = RiskLevel::Warning;
    const DANGER: RiskLevel = RiskLevel::Danger;

    #[test]
    fn risk_levels_are_ordered_by_severity() {
        assert!(SAFE < WARNING && WARNING < DANGER);
    }

    #[test]
    fn clean_token_is_safe() {
        let (risk, factors) =
            assess_risk(&supply(1_000), &authority(SAFE, SAFE), None, Some(&holders(10.0, SAFE)));
        assert_eq!(risk, SAFE);
        assert!(factors.is_empty());
    }

    #[test]
    fn warnings_are_listed_even_when_a_danger_was_already_found() {
        let (risk, factors) = assess_risk(
            &supply(1_000),
            &authority(DANGER, SAFE),
            None,
            Some(&holders(60.0, WARNING)),
        );
        assert_eq!(risk, DANGER);
        assert_eq!(
            factors,
            [
                "Mint authority is active - unlimited tokens can be minted",
                "Moderate concentration: top 10 token accounts hold 60.0% of the supply",
            ]
        );
    }

    #[test]
    fn freeze_authority_alone_is_danger() {
        let (risk, factors) =
            assess_risk(&supply(1_000), &authority(SAFE, DANGER), None, Some(&holders(10.0, SAFE)));
        assert_eq!(risk, DANGER);
        assert_eq!(factors, ["Freeze authority is active - accounts can be frozen"]);
    }

    #[test]
    fn high_concentration_alone_is_danger() {
        let (risk, factors) =
            assess_risk(&supply(1_000), &authority(SAFE, SAFE), None, Some(&holders(95.0, DANGER)));
        assert_eq!(risk, DANGER);
        assert_eq!(
            factors,
            ["High concentration: top 10 token accounts hold 95.0% of the supply"]
        );
    }

    #[test]
    fn moderate_concentration_alone_is_warning() {
        let (risk, factors) =
            assess_risk(&supply(1_000), &authority(SAFE, SAFE), None, Some(&holders(60.0, WARNING)));
        assert_eq!(risk, WARNING);
        assert_eq!(factors.len(), 1);
        assert!(factors[0].starts_with("Moderate concentration"));
    }

    #[test]
    fn missing_holder_data_is_not_safe() {
        let (risk, factors) = assess_risk(&supply(1_000), &authority(SAFE, SAFE), None, None);
        assert_eq!(risk, WARNING);
        assert_eq!(factors, ["Holder concentration could not be checked"]);
    }

    #[test]
    fn zero_supply_is_flagged() {
        let (risk, factors) =
            assess_risk(&supply(0), &authority(SAFE, SAFE), None, Some(&holders(0.0, SAFE)));
        assert_eq!(risk, WARNING);
        assert_eq!(factors, ["Total supply is zero"]);
    }

    #[test]
    fn extension_findings_are_included() {
        let extensions = ExtensionsCheck {
            extensions: vec!["PermanentDelegate".to_string()],
            risk: DANGER,
            risk_factors: vec!["Permanent delegate".to_string()],
        };
        let (risk, factors) = assess_risk(
            &supply(1_000),
            &authority(SAFE, SAFE),
            Some(&extensions),
            Some(&holders(10.0, SAFE)),
        );
        assert_eq!(risk, DANGER);
        assert_eq!(factors, ["Permanent delegate"]);

        // Every extension finding is carried over, in order, after the authorities.
        let several = ExtensionsCheck {
            extensions: Vec::new(),
            risk: WARNING,
            risk_factors: vec!["Transfer hook".to_string(), "Transfer fee".to_string()],
        };
        let (risk, factors) = assess_risk(
            &supply(1_000),
            &authority(DANGER, SAFE),
            Some(&several),
            Some(&holders(10.0, SAFE)),
        );
        assert_eq!(risk, DANGER);
        assert_eq!(factors.len(), 3);
        assert_eq!(factors[1..], ["Transfer hook", "Transfer fee"]);

        // Extensions present but harmless contribute nothing.
        let harmless = ExtensionsCheck {
            extensions: vec!["TokenMetadata".to_string()],
            risk: SAFE,
            risk_factors: Vec::new(),
        };
        let (risk, factors) = assess_risk(
            &supply(1_000),
            &authority(SAFE, SAFE),
            Some(&harmless),
            Some(&holders(10.0, SAFE)),
        );
        assert_eq!(risk, SAFE);
        assert!(factors.is_empty());
    }
}
