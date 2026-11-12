use spl_token_2022::state::Mint;

use crate::types::SupplyInfo;

pub fn supply_info(mint: &Mint) -> SupplyInfo {
    SupplyInfo {
        total_supply: mint.supply,
        decimals: mint.decimals,
        formatted_supply: format_supply(mint.supply, mint.decimals),
    }
}

/// Renders a raw token amount as a decimal number with thousands separators.
///
/// Works on the digit string instead of dividing by `10^decimals`: `decimals`
/// is an arbitrary on-chain `u8`, and `10u64.pow(20)` already overflows.
fn format_supply(supply: u64, decimals: u8) -> String {
    let decimals = usize::from(decimals);
    // Left-pad so there is always at least one digit before the decimal point.
    let digits = format!("{supply:0>width$}", width = decimals + 1);
    let (whole, frac) = digits.split_at(digits.len() - decimals);
    let whole = group_thousands(whole);
    let frac = frac.trim_end_matches('0');

    if frac.is_empty() {
        whole
    } else {
        format!("{whole}.{frac}")
    }
}

fn group_thousands(digits: &str) -> String {
    let mut result = String::with_capacity(digits.len() + digits.len() / 3);
    // A separator goes wherever the number of remaining digits is a multiple of 3.
    let phase = digits.len() % 3;
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && i % 3 == phase {
            result.push(',');
        }
        result.push(c);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supply_info_applies_the_mint_decimals() {
        let info = supply_info(&Mint {
            supply: 1_500_000_000,
            decimals: 9,
            is_initialized: true,
            ..Mint::default()
        });
        assert_eq!(info.total_supply, 1_500_000_000);
        assert_eq!(info.decimals, 9);
        assert_eq!(info.formatted_supply, "1.5");
    }

    #[test]
    fn formats_whole_amounts_with_separators() {
        assert_eq!(format_supply(0, 0), "0");
        assert_eq!(format_supply(999, 0), "999");
        assert_eq!(format_supply(1_000, 0), "1,000");
        assert_eq!(format_supply(1_234_567, 0), "1,234,567");
        assert_eq!(format_supply(u64::MAX, 0), "18,446,744,073,709,551,615");
    }

    #[test]
    fn formats_fractions_and_trims_trailing_zeros() {
        assert_eq!(format_supply(0, 9), "0");
        assert_eq!(format_supply(1, 9), "0.000000001");
        assert_eq!(format_supply(1_500_000_000, 9), "1.5");
        assert_eq!(format_supply(1_000_000_000_000_000, 6), "1,000,000,000");
        assert_eq!(format_supply(123_456_789, 2), "1,234,567.89");
    }

    #[test]
    fn handles_decimals_beyond_u64_range() {
        // 10^20 does not fit in a u64; these used to overflow.
        assert_eq!(format_supply(1, 20), "0.00000000000000000001");
        assert_eq!(format_supply(u64::MAX, 20), "0.18446744073709551615");
        assert_eq!(format_supply(5, 255), format!("0.{}5", "0".repeat(254)));
    }
}
