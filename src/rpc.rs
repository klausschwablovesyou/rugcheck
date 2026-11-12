use std::future::Future;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde_json::json;
use solana_account_decoder::UiAccountEncoding;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_api::{
    client_error::{Error as ClientError, ErrorKind as ClientErrorKind},
    config::RpcAccountInfoConfig,
    request::RpcRequest,
    response::{Response, RpcTokenAccountBalance},
};
use solana_sdk::{account::Account, commitment_config::CommitmentConfig, pubkey::Pubkey};

pub struct SolanaRpc {
    client: RpcClient,
    timeout: Duration,
}

impl SolanaRpc {
    pub fn new(rpc_url: &str, timeout: Duration) -> Self {
        Self {
            client: RpcClient::new_with_timeout_and_commitment(
                rpc_url.to_string(),
                timeout,
                CommitmentConfig::finalized(),
            ),
            timeout,
        }
    }

    /// Runs a request with a bound on its total time. The HTTP timeout alone
    /// does not give one: on HTTP 429 the client sleeps for the server's
    /// `Retry-After` and retries up to five times, which can take minutes.
    async fn request<T>(&self, request: impl Future<Output = Result<T, ClientError>>) -> Result<T> {
        tokio::time::timeout(self.timeout, request)
            .await
            .map_err(|_| {
                anyhow!(
                    "No response within {} seconds (the endpoint may be rate limiting this request)",
                    self.timeout.as_secs()
                )
            })?
            .map_err(without_url)
    }

    /// Fetches several accounts in a single `getMultipleAccounts` round trip.
    /// The result has one entry per requested address, `None` where the
    /// account does not exist.
    ///
    /// Uses the `_with_config` variant on purpose: the convenience wrappers
    /// first issue a hidden `getVersion` request to map commitment levels for
    /// pre-1.5.5 nodes.
    pub async fn get_accounts<const N: usize>(&self, addresses: &[Pubkey; N]) -> Result<[Option<Account>; N]> {
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            commitment: Some(self.client.commitment()),
            ..RpcAccountInfoConfig::default()
        };

        self.request(self.client.get_multiple_accounts_with_config(addresses, config))
            .await
            .context("Failed to fetch accounts")?
            .value
            .try_into()
            .map_err(|accounts: Vec<_>| {
                anyhow!("RPC returned {} accounts for {N} requested addresses", accounts.len())
            })
    }

    /// Returns the largest token accounts of a mint (the RPC caps the list at
    /// 20) as `(token account address, raw amount)`, largest first.
    pub async fn get_largest_token_accounts(&self, mint_address: &Pubkey) -> Result<Vec<(Pubkey, u64)>> {
        // Sent as a raw request for the same reason as above: no `getVersion` probe.
        let response: Response<Vec<RpcTokenAccountBalance>> = self
            .request(self.client.send(
                RpcRequest::GetTokenLargestAccounts,
                json!([mint_address.to_string(), self.client.commitment()]),
            ))
            .await
            .context("Failed to fetch largest token accounts")?;

        response
            .value
            .into_iter()
            .map(|account| {
                // Parsed rather than passed through: the address is printed
                // in the report, so it must be a real address.
                let address = account
                    .address
                    .parse::<Pubkey>()
                    .context("RPC returned an invalid token account address")?;
                let amount = account
                    .amount
                    .amount
                    .parse::<u64>()
                    .with_context(|| format!("Invalid token amount for account {address}"))?;
                Ok((address, amount))
            })
            .collect()
    }
}

/// Transport errors quote the request URL, and the URL of a hosted RPC
/// endpoint usually contains the API token: keep it out of the output.
fn without_url(error: ClientError) -> anyhow::Error {
    match error.kind {
        ClientErrorKind::Reqwest(error) => anyhow!(error.without_url()),
        kind => anyhow!(kind),
    }
}
