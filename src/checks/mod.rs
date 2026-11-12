mod authority;
mod extensions;
mod holders;
mod metadata;
mod supply;

use anyhow::{anyhow, bail, Context, Result};
use solana_sdk::{
    account::{from_account, Account},
    clock::Clock,
    pubkey::Pubkey,
    sysvar,
};
use spl_token_2022::{
    extension::{BaseStateWithExtensions, StateWithExtensions},
    state::Mint,
};

use crate::rpc::SolanaRpc;
use crate::types::RugCheckResult;

pub async fn run_all_checks(rpc: &SolanaRpc, mint_address: &Pubkey) -> Result<RugCheckResult> {
    // Everything the report needs comes from two independent requests, so
    // they are sent concurrently: one batch of accounts (the mint, its
    // Metaplex metadata, and the clock for the current epoch), and the
    // largest token accounts.
    let addresses = [
        *mint_address,
        metadata::metadata_pda(mint_address),
        sysvar::clock::id(),
    ];
    // Without a valid mint there is nothing to report on. `try_join!` returns
    // that error as soon as the account batch shows it, without waiting for
    // the holders request; a failed holders request alone is not fatal, so
    // its result is passed through as a value.
    let ((mint_account, metadata_account, clock_account), largest_accounts) = tokio::try_join!(
        async {
            let [mint_account, metadata_account, clock_account] = rpc.get_accounts(&addresses).await?;
            let mint_account =
                mint_account.with_context(|| format!("Account {mint_address} does not exist"))?;
            unpack_mint(mint_address, &mint_account)?;
            Ok((mint_account, metadata_account, clock_account))
        },
        async { anyhow::Ok(rpc.get_largest_token_accounts(mint_address).await) },
    )?;

    build_report(
        mint_address,
        &mint_account,
        metadata_account.as_ref(),
        clock_account.as_ref(),
        largest_accounts,
    )
}

/// Turns the fetched data into the report. Kept free of I/O (apart from
/// warnings on stderr) so the whole decision logic can be tested offline.
fn build_report(
    mint_address: &Pubkey,
    mint_account: &Account,
    metadata_account: Option<&Account>,
    clock_account: Option<&Account>,
    largest_accounts: Result<Vec<(Pubkey, u64)>>,
) -> Result<RugCheckResult> {
    let mint = unpack_mint(mint_address, mint_account)?;
    let extension_data = mint.get_tlv_data();
    let is_token_2022 = mint_account.owner == spl_token_2022::id();
    let current_epoch = clock_account
        .and_then(from_account::<Clock, _>)
        .map(|clock| clock.epoch);

    let holders = match largest_accounts {
        // Every token of a non-zero supply sits in some account, so a list
        // without balances means the node has no holder data for this mint -
        // which must not be reported as "0% concentration".
        Ok(accounts) if mint.base.supply > 0 && accounts.iter().all(|(_, amount)| *amount == 0) => {
            eprintln!("Warning: Failed to check holders: the RPC returned no token balances for this mint");
            None
        }
        Ok(accounts) => Some(holders::check_holders(accounts, mint.base.supply)),
        Err(e) => {
            // The client error repeats its source in its own message, so
            // print the outer context and the root cause instead of the chain.
            eprintln!("Warning: Failed to check holders: {e}: {}", e.root_cause());
            None
        }
    };

    let metadata = metadata_account
        .and_then(metadata::parse_metaplex_account)
        .or_else(|| metadata::from_mint_extension(extension_data));

    Ok(RugCheckResult::new(
        mint_address,
        if is_token_2022 { "spl-token-2022" } else { "spl-token" },
        metadata,
        supply::supply_info(&mint.base),
        authority::check_authorities(&mint.base),
        extensions::check_extensions(extension_data, current_epoch),
        holders,
    ))
}

/// Parses a mint of either token program: a legacy SPL Token mint is the same
/// 82 byte layout with no extension data after it. Fails for anything that is
/// not an initialized mint (wallets, token accounts, multisigs, ...).
fn unpack_mint<'a>(mint_address: &Pubkey, account: &'a Account) -> Result<StateWithExtensions<'a, Mint>> {
    if spl_token_2022::check_spl_token_program_account(&account.owner).is_err() {
        bail!(
            "{mint_address} is not a token mint (it is owned by program {})",
            account.owner
        );
    }
    StateWithExtensions::<Mint>::unpack(&account.data)
        .map_err(|e| anyhow!("{mint_address} is not a valid token mint: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RiskLevel;
    use solana_sdk::{program_pack::Pack, pubkey};
    use spl_token_2022::state::{Account as TokenAccount, AccountState};

    const LEGACY_TOKEN_PROGRAM: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
    const TOKEN_2022_PROGRAM: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

    fn mint_account(owner: Pubkey, supply: u64) -> Account {
        let mut data = vec![0; Mint::LEN];
        Mint {
            supply,
            decimals: 6,
            is_initialized: true,
            ..Mint::default()
        }
        .pack_into_slice(&mut data);
        Account {
            data,
            owner,
            ..Account::default()
        }
    }

    /// Ten accounts holding 1% of a supply of 1,000 each.
    fn spread_out_holders() -> Result<Vec<(Pubkey, u64)>> {
        Ok((0..10).map(|_| (Pubkey::new_unique(), 10)).collect())
    }

    fn report(mint_account: &Account, largest_accounts: Result<Vec<(Pubkey, u64)>>) -> Result<RugCheckResult> {
        build_report(&Pubkey::new_unique(), mint_account, None, None, largest_accounts)
    }

    #[test]
    fn clean_legacy_mint_is_safe() {
        let mint_address = Pubkey::new_unique();
        let account = mint_account(LEGACY_TOKEN_PROGRAM, 1_000);
        let result = build_report(&mint_address, &account, None, None, spread_out_holders()).unwrap();

        assert_eq!(result.token_address, mint_address.to_string());
        assert_eq!(result.token_program, "spl-token");
        assert_eq!(result.supply.total_supply, 1_000);
        assert_eq!(result.supply.formatted_supply, "0.001");
        assert!(result.metadata.is_none());
        assert!(result.extensions.is_none());
        assert_eq!(result.holders.unwrap().top_10_concentration, 10.0);
        assert_eq!(result.overall_risk, RiskLevel::Safe);
        assert!(result.risk_factors.is_empty());
        assert_eq!(result.not_checked, ["liquidity"]);
    }

    #[test]
    fn token_2022_mint_is_labelled() {
        let account = mint_account(TOKEN_2022_PROGRAM, 1_000);
        let result = report(&account, spread_out_holders()).unwrap();
        assert_eq!(result.token_program, "spl-token-2022");
        assert!(result.extensions.is_none());
    }

    #[test]
    fn account_of_another_program_is_rejected() {
        // The bytes are a perfectly valid mint; only the owner is wrong.
        let account = mint_account(Pubkey::new_unique(), 1_000);
        let error = report(&account, spread_out_holders()).unwrap_err();
        assert!(error.to_string().contains("is not a token mint"), "{error}");
    }

    #[test]
    fn token_account_is_not_a_mint() {
        let mut data = vec![0; TokenAccount::LEN];
        TokenAccount {
            mint: Pubkey::new_unique(),
            owner: Pubkey::new_unique(),
            amount: 5,
            state: AccountState::Initialized,
            ..TokenAccount::default()
        }
        .pack_into_slice(&mut data);

        for owner in [LEGACY_TOKEN_PROGRAM, TOKEN_2022_PROGRAM] {
            let account = Account {
                data: data.clone(),
                owner,
                ..Account::default()
            };
            let error = report(&account, spread_out_holders()).unwrap_err();
            assert!(error.to_string().contains("is not a valid token mint"), "{error}");
        }
    }

    #[test]
    fn uninitialized_mint_is_rejected() {
        let account = Account {
            data: vec![0; Mint::LEN],
            owner: LEGACY_TOKEN_PROGRAM,
            ..Account::default()
        };
        assert!(report(&account, spread_out_holders()).is_err());
    }

    #[test]
    fn failed_holders_request_is_not_fatal_but_not_safe() {
        let account = mint_account(LEGACY_TOKEN_PROGRAM, 1_000);
        let result = report(&account, Err(anyhow!("HTTP 429"))).unwrap();
        assert!(result.holders.is_none());
        assert_eq!(result.overall_risk, RiskLevel::Warning);
        assert_eq!(result.risk_factors, ["Holder concentration could not be checked"]);
        assert_eq!(result.not_checked, ["liquidity", "holders"]);
    }

    #[test]
    fn holder_list_without_balances_is_not_reported_as_zero_concentration() {
        let account = mint_account(LEGACY_TOKEN_PROGRAM, 1_000);
        for accounts in [Vec::new(), vec![(Pubkey::new_unique(), 0)]] {
            let result = report(&account, Ok(accounts)).unwrap();
            assert!(result.holders.is_none());
            assert_eq!(result.overall_risk, RiskLevel::Warning);
        }

        // With a zero supply an empty list is the truth.
        let empty_mint = mint_account(LEGACY_TOKEN_PROGRAM, 0);
        let result = report(&empty_mint, Ok(Vec::new())).unwrap();
        assert!(result.holders.is_some());
        assert_eq!(result.risk_factors, ["Total supply is zero"]);
    }

    #[test]
    fn metadata_falls_back_to_the_mint_extension() {
        // A Token-2022 mint: the base padded to the token account size, the
        // account type byte (1 = mint), then one TokenMetadata (19) entry.
        let mut value = vec![0; 64]; // update authority + mint
        for field in ["Ext Coin", "EXT", "https://example.com/ext.json"] {
            value.extend_from_slice(&(field.len() as u32).to_le_bytes());
            value.extend_from_slice(field.as_bytes());
        }
        let mut account = mint_account(TOKEN_2022_PROGRAM, 1_000);
        account.data.resize(TokenAccount::LEN, 0);
        account.data.push(1);
        account.data.extend_from_slice(&19u16.to_le_bytes());
        account.data.extend_from_slice(&(value.len() as u16).to_le_bytes());
        account.data.extend_from_slice(&value);

        let result = report(&account, spread_out_holders()).unwrap();
        let metadata = result.metadata.unwrap();
        assert_eq!(metadata.name, "Ext Coin");
        assert_eq!(metadata.symbol, "EXT");
        assert_eq!(result.extensions.unwrap().extensions, ["TokenMetadata"]);
        assert_eq!(result.overall_risk, RiskLevel::Safe);
    }

    #[test]
    fn metaplex_metadata_wins_over_the_mint_extension() {
        let mut data = vec![4];
        data.extend_from_slice(&[0; 64]);
        for field in ["Name", "SYM", "uri"] {
            data.extend_from_slice(&(field.len() as u32).to_le_bytes());
            data.extend_from_slice(field.as_bytes());
        }
        let metadata_account = Account {
            data,
            owner: pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"),
            ..Account::default()
        };
        let account = mint_account(LEGACY_TOKEN_PROGRAM, 1_000);

        let result = build_report(
            &Pubkey::new_unique(),
            &account,
            Some(&metadata_account),
            None,
            spread_out_holders(),
        )
        .unwrap();
        assert_eq!(result.metadata.unwrap().symbol, "SYM");
    }
}
