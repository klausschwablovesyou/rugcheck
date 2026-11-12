use solana_sdk::pubkey::Pubkey;

use crate::types::{HolderInfo, HoldersCheck, RiskLevel};

const TOP_HOLDERS: usize = 10;

/// Evaluates how concentrated the supply is in the largest token accounts.
///
/// `largest_accounts` are token accounts (not wallets), largest first.
pub fn check_holders(largest_accounts: Vec<(Pubkey, u64)>, total_supply: u64) -> HoldersCheck {
    let percentage_of_supply = |amount: u128| {
        if total_supply > 0 {
            amount as f64 / total_supply as f64 * 100.0
        } else {
            0.0
        }
    };

    let top_holders: Vec<HolderInfo> = largest_accounts
        .into_iter()
        .take(TOP_HOLDERS)
        .map(|(address, balance)| HolderInfo {
            address: address.to_string(),
            balance,
            percentage: percentage_of_supply(balance.into()),
        })
        .collect();

    // Sum the raw balances rather than the rounded percentages.
    let top_10_balance: u128 = top_holders.iter().map(|h| u128::from(h.balance)).sum();
    let top_10_concentration = percentage_of_supply(top_10_balance);

    HoldersCheck {
        top_holders,
        top_10_concentration,
        risk: evaluate_concentration(top_10_concentration),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn accounts(balances: &[u64]) -> Vec<(Pubkey, u64)> {
        balances
            .iter()
            .map(|&balance| (Pubkey::new_unique(), balance))
            .collect()
    }

    #[test]
    fn concentration_thresholds() {
        assert_eq!(evaluate_concentration(0.0), RiskLevel::Safe);
        assert_eq!(evaluate_concentration(49.99), RiskLevel::Safe);
        assert_eq!(evaluate_concentration(50.0), RiskLevel::Warning);
        assert_eq!(evaluate_concentration(79.99), RiskLevel::Warning);
        assert_eq!(evaluate_concentration(80.0), RiskLevel::Danger);
        assert_eq!(evaluate_concentration(100.0), RiskLevel::Danger);
    }

    #[test]
    fn only_the_top_ten_accounts_count() {
        // The RPC returns up to 20 accounts; 12 accounts of 5% each.
        let largest = accounts(&[5; 12]);
        let check = check_holders(largest.clone(), 100);
        assert_eq!(check.top_holders.len(), 10);
        assert_eq!(check.top_10_concentration, 50.0);
        assert_eq!(check.risk, RiskLevel::Warning);
        for (holder, (address, _)) in check.top_holders.iter().zip(&largest) {
            assert_eq!(holder.address, address.to_string());
            assert_eq!(holder.balance, 5);
            assert_eq!(holder.percentage, 5.0);
        }
    }

    #[test]
    fn concentration_is_computed_from_raw_balances() {
        // Summing the three rounded percentages gives 49.99999999999999.
        let check = check_holders(accounts(&[1, 1, 1]), 6);
        assert_eq!(check.top_10_concentration, 50.0);
        assert_eq!(check.risk, RiskLevel::Warning);
    }

    #[test]
    fn supply_of_one() {
        let check = check_holders(accounts(&[1]), 1);
        assert_eq!(check.top_10_concentration, 100.0);
        assert_eq!(check.risk, RiskLevel::Danger);
    }

    #[test]
    fn full_supply_in_one_account_is_exactly_100_percent() {
        let check = check_holders(accounts(&[u64::MAX]), u64::MAX);
        assert_eq!(check.top_10_concentration, 100.0);
        assert_eq!(check.risk, RiskLevel::Danger);
    }

    #[test]
    fn zero_supply_does_not_divide_by_zero() {
        let check = check_holders(accounts(&[0, 0]), 0);
        assert_eq!(check.top_10_concentration, 0.0);
        assert_eq!(check.top_holders[0].percentage, 0.0);
    }

    #[test]
    fn no_accounts() {
        let check = check_holders(Vec::new(), 1_000);
        assert!(check.top_holders.is_empty());
        assert_eq!(check.top_10_concentration, 0.0);
        assert_eq!(check.risk, RiskLevel::Safe);
    }
}
