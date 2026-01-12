use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_request::TokenAccountsFilter;
use solana_sdk::pubkey::Pubkey;
use spl_token::state::Mint;
use solana_sdk::program_pack::Pack;

pub struct SolanaRpc {
    client: RpcClient,
}

impl SolanaRpc {
    pub fn new(rpc_url: &str) -> Self {
        Self {
            client: RpcClient::new(rpc_url.to_string()),
        }
    }

    pub fn get_mint_info(&self, mint_address: &Pubkey) -> Result<Mint> {
        let account = self.client
            .get_account(mint_address)
            .context("Failed to fetch mint account")?;

        Mint::unpack(&account.data)
            .context("Failed to parse mint data")
    }

    pub fn get_largest_token_accounts(&self, mint_address: &Pubkey) -> Result<Vec<(Pubkey, u64)>> {
        let accounts = self.client
            .get_token_largest_accounts(mint_address)
            .context("Failed to fetch largest token accounts")?;

        let result: Vec<(Pubkey, u64)> = accounts
            .value
            .into_iter()
            .filter_map(|acc| {
                let pubkey = acc.address.parse::<Pubkey>().ok()?;
                let amount = acc.amount.parse::<u64>().ok()?;
                Some((pubkey, amount))
            })
            .collect();

        Ok(result)
    }

    pub fn get_token_accounts_by_owner(&self, owner: &Pubkey, mint: &Pubkey) -> Result<Vec<Pubkey>> {
        let accounts = self.client
            .get_token_accounts_by_owner(owner, TokenAccountsFilter::Mint(*mint))
            .context("Failed to fetch token accounts by owner")?;

        let result: Vec<Pubkey> = accounts
            .into_iter()
            .filter_map(|acc| acc.pubkey.parse::<Pubkey>().ok())
            .collect();

        Ok(result)
    }

    pub fn get_account_data(&self, address: &Pubkey) -> Result<Vec<u8>> {
        let account = self.client
            .get_account(address)
            .context("Failed to fetch account")?;

        Ok(account.data)
    }

    pub fn account_exists(&self, address: &Pubkey) -> bool {
        self.client.get_account(address).is_ok()
    }

    pub fn inner(&self) -> &RpcClient {
        &self.client
    }
}
