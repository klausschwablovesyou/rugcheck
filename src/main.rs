mod checks;
mod rpc;
mod types;

use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use colored::{ColoredString, Colorize};
use solana_sdk::pubkey::Pubkey;

use checks::run_all_checks;
use rpc::SolanaRpc;
use types::{RiskLevel, RugCheckResult};

#[derive(Parser)]
#[command(name = "rugcheck")]
#[command(about = "Check Solana tokens for rug pull indicators", long_about = None)]
struct Cli {
    /// The token mint address to check
    token_address: Pubkey,

    /// QuickNode RPC URL (or set QUICKNODE_RPC_URL env var)
    // The URL usually embeds the API token, so keep its value out of `--help`.
    #[arg(short, long, env = "QUICKNODE_RPC_URL", hide_env_values = true)]
    rpc_url: String,

    /// Timeout for each RPC request, in seconds
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..))]
    timeout: u64,

    /// Output results as JSON
    #[arg(long)]
    json: bool,
}

// The work is two concurrent HTTP requests; a single-threaded runtime is plenty.
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let rpc = SolanaRpc::new(&cli.rpc_url, Duration::from_secs(cli.timeout));

    let result = run_all_checks(&rpc, &cli.token_address).await?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print_result(&result);
    }

    Ok(())
}

const RULE: &str = "═══════════════════════════════════════════════════════════════";

fn print_result(result: &RugCheckResult) {
    println!();
    println!("{}", RULE.bright_white());
    println!("{}", "                    SOLANA TOKEN RUG CHECK                      ".bright_white().bold());
    println!("{}", RULE.bright_white());
    println!();

    println!("{}:   {}", "Token".bright_white().bold(), result.token_address);
    println!("{}: {}", "Program".bright_white().bold(), result.token_program);
    println!();

    if let Some(meta) = &result.metadata {
        println!("{}", "─── Token Info ───".bright_white());
        for (label, value) in [("Name:  ", &meta.name), ("Symbol:", &meta.symbol), ("URI:   ", &meta.uri)] {
            if !value.is_empty() {
                println!("  {label} {value}");
            }
        }
        println!();
    }

    println!("{}", "─── Supply ───".bright_white());
    println!("  Total:    {}", result.supply.formatted_supply);
    println!("  Decimals: {}", result.supply.decimals);
    println!();

    println!("{}", "─── Authority Check ───".bright_white());
    print!("  Mint Authority:   ");
    print_authority_status(result.authority.mint_authority.as_deref(), result.authority.mint_authority_risk);
    print!("  Freeze Authority: ");
    print_authority_status(result.authority.freeze_authority.as_deref(), result.authority.freeze_authority_risk);
    println!();

    if let Some(extensions) = &result.extensions {
        println!("{}", "─── Token-2022 Extensions ───".bright_white());
        println!("  Risk: {}", format_risk(extensions.risk));
        if !extensions.extensions.is_empty() {
            println!("  {}", extensions.extensions.join(", "));
        }
        println!();
    }

    println!("{}", "─── Largest Token Accounts ───".bright_white());
    match &result.holders {
        Some(holders) => {
            println!(
                "  Top 10 Concentration: {:.1}% {}",
                holders.top_10_concentration,
                format_risk(holders.risk)
            );
            println!("  {}", "(token accounts, not wallets - may include DEX pools or burn addresses)".dimmed());
            println!();
            for (i, holder) in holders.top_holders.iter().take(5).enumerate() {
                println!("  {}. {} ({:.2}%)", i + 1, holder.address, holder.percentage);
            }
        }
        None => println!("  {}", "Not available".yellow()),
    }
    println!();

    println!("{}", "─── Liquidity ───".bright_white());
    println!("  {}", "Not checked - verify pools and LP lock status manually".dimmed());
    println!();

    println!("{}", RULE.bright_white());
    println!("  OVERALL RISK: {}", risk_label(result.overall_risk).bold());

    if !result.risk_factors.is_empty() {
        println!();
        println!("{}", "  Risk Factors:".bright_white());
        for factor in &result.risk_factors {
            println!("    {} {}", "•".red(), factor);
        }
    }

    println!("{}", RULE.bright_white());
    println!();
}

fn print_authority_status(authority: Option<&str>, risk: RiskLevel) {
    match authority {
        Some(address) => println!("{} {} {}", address, format_risk(risk), "(Active)".red()),
        None => println!("{} {}", "Revoked".green(), format_risk(risk)),
    }
}

fn risk_label(risk: RiskLevel) -> ColoredString {
    let label = risk.as_str();
    match risk {
        RiskLevel::Safe => label.green(),
        RiskLevel::Warning => label.yellow(),
        RiskLevel::Danger => label.red(),
    }
}

fn format_risk(risk: RiskLevel) -> String {
    format!("[{}]", risk_label(risk))
}
