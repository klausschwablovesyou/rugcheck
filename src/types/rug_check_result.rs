use serde::Serialize;
use solana_sdk::pubkey::Pubkey;
use super::authority_check::AuthorityCheck;
use super::holders_check::HoldersCheck;
use super::liquidity_check::LiquidityCheck;
use super::risk_level::RiskLevel;
use super::supply_info::SupplyInfo;
use super::token_metadata::TokenMetadata;

#[derive(Debug, Clone, Serialize)]
pub struct RugCheckResult {
    pub token_address: String,
    pub metadata: Option<TokenMetadata>,
    pub supply: Option<SupplyInfo>,
    pub authority: Option<AuthorityCheck>,
    pub holders: Option<HoldersCheck>,
    pub liquidity: Option<LiquidityCheck>,
    pub overall_risk: RiskLevel,
    pub risk_factors: Vec<String>,
}

impl RugCheckResult {
    pub fn new(token_address: Pubkey) -> Self {
        Self {
            token_address: token_address.to_string(),
            metadata: None,
            supply: None,
            authority: None,
            holders: None,
            liquidity: None,
            overall_risk: RiskLevel::Safe,
            risk_factors: Vec::new(),
        }
    }

    pub fn calculate_overall_risk(&mut self) {
        let mut highest_risk = RiskLevel::Safe;

        if let Some(ref auth) = self.authority {
            if auth.mint_authority_risk == RiskLevel::Danger {
                highest_risk = RiskLevel::Danger;
                self.risk_factors.push("Mint authority is active - unlimited tokens can be minted".to_string());
            }
            if auth.freeze_authority_risk == RiskLevel::Danger {
                highest_risk = RiskLevel::Danger;
                self.risk_factors.push("Freeze authority is active - accounts can be frozen".to_string());
            }
        }

        if let Some(ref holders) = self.holders {
            if holders.risk == RiskLevel::Danger {
                if highest_risk != RiskLevel::Danger {
                    highest_risk = RiskLevel::Danger;
                }
                self.risk_factors.push(format!(
                    "High concentration: top 10 holders own {:.1}%",
                    holders.top_10_concentration
                ));
            } else if holders.risk == RiskLevel::Warning && highest_risk == RiskLevel::Safe {
                highest_risk = RiskLevel::Warning;
                self.risk_factors.push(format!(
                    "Moderate concentration: top 10 holders own {:.1}%",
                    holders.top_10_concentration
                ));
            }
        }

        if let Some(ref liq) = self.liquidity {
            if liq.risk == RiskLevel::Danger {
                if highest_risk != RiskLevel::Danger {
                    highest_risk = RiskLevel::Danger;
                }
                self.risk_factors.push(liq.details.clone());
            } else if liq.risk == RiskLevel::Warning && highest_risk == RiskLevel::Safe {
                highest_risk = RiskLevel::Warning;
                self.risk_factors.push(liq.details.clone());
            }
        }

        self.overall_risk = highest_risk;
    }
}
