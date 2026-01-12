mod checks;
mod rpc;
mod types;

use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

use checks::run_all_checks;
use rpc::SolanaRpc;
use types::RiskLevel;

#[derive(Parser)]
#[command(name = "rugcheck")]
#[command(about = "Check Solana tokens for rug pull indicators", long_about = None)]
struct Cli {
    /// The token mint address to check
    token_address: String,

    /// QuickNode RPC URL (or set QUICKNODE_RPC_URL env var)
    #[arg(short, long, env = "QUICKNODE_RPC_URL")]
    rpc_url: String,

    /// Output results as JSON
    #[arg(long, default_value = "false")]
    json: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let mint_address = Pubkey::from_str(&cli.token_address)
        .context("Invalid token address")?;

    let rpc = SolanaRpc::new(&cli.rpc_url);

    let result = run_all_checks(&rpc, &mint_address)?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print_result(&result);
    }

    Ok(())
}

fn print_result(result: &types::RugCheckResult) {
    println!();
    println!("{}", "═══════════════════════════════════════════════════════════════".bright_white());
    println!("{}", "                    SOLANA TOKEN RUG CHECK                      ".bright_white().bold());
    println!("{}", "═══════════════════════════════════════════════════════════════".bright_white());
    println!();

    // Token Address
    println!("{}: {}", "Token".bright_white().bold(), result.token_address);
    println!();

    // Metadata
    if let Some(ref meta) = result.metadata {
        println!("{}", "─── Token Info ───".bright_white());
        if let Some(ref name) = meta.name {
            if !name.is_empty() {
                println!("  Name:   {}", name);
            }
        }
        if let Some(ref symbol) = meta.symbol {
            if !symbol.is_empty() {
                println!("  Symbol: {}", symbol);
            }
        }
        if let Some(ref uri) = meta.uri {
            if !uri.is_empty() {
                println!("  URI:    {}", uri);
            }
        }
        println!();
    }

    // Supply
    if let Some(ref supply) = result.supply {
        println!("{}", "─── Supply ───".bright_white());
        println!("  Total:    {}", supply.formatted_supply);
        println!("  Decimals: {}", supply.decimals);
        println!();
    }

    // Authority Check
    if let Some(ref auth) = result.authority {
        println!("{}", "─── Authority Check ───".bright_white());
        print!("  Mint Authority:   ");
        print_authority_status(&auth.mint_authority, auth.mint_authority_risk);
        print!("  Freeze Authority: ");
        print_authority_status(&auth.freeze_authority, auth.freeze_authority_risk);
        println!();
    }

    // Holders Check
    if let Some(ref holders) = result.holders {
        println!("{}", "─── Top Holders ───".bright_white());
        println!(
            "  Top 10 Concentration: {} {}",
            format!("{:.1}%", holders.top_10_concentration),
            format_risk(holders.risk)
        );
        println!();
        for (i, holder) in holders.top_holders.iter().take(5).enumerate() {
            println!(
                "  {}. {} ({:.2}%)",
                i + 1,
                truncate_address(&holder.address),
                holder.percentage
            );
        }
        println!();
    }

    // Liquidity Check
    if let Some(ref liq) = result.liquidity {
        println!("{}", "─── Liquidity ───".bright_white());
        println!(
            "  Status: {} {}",
            if liq.has_liquidity { "Found" } else { "Not Found" },
            format_risk(liq.risk)
        );
        println!("  {}", liq.details);
        println!();
    }

    // Overall Risk
    println!("{}", "═══════════════════════════════════════════════════════════════".bright_white());
    print!("  OVERALL RISK: ");
    match result.overall_risk {
        RiskLevel::Safe => println!("{}", "SAFE".green().bold()),
        RiskLevel::Warning => println!("{}", "WARNING".yellow().bold()),
        RiskLevel::Danger => println!("{}", "DANGER".red().bold()),
    }

    if !result.risk_factors.is_empty() {
        println!();
        println!("{}", "  Risk Factors:".bright_white());
        for factor in &result.risk_factors {
            println!("    {} {}", "•".red(), factor);
        }
    }

    println!("{}", "═══════════════════════════════════════════════════════════════".bright_white());
    println!();
}

fn print_authority_status(authority: &Option<String>, risk: RiskLevel) {
    match authority {
        Some(addr) => {
            println!(
                "{} {} {}",
                truncate_address(addr),
                format_risk(risk),
                "(Active)".red()
            );
        }
        None => {
            println!("{} {}", "Revoked".green(), format_risk(risk));
        }
    }
}

fn format_risk(risk: RiskLevel) -> String {
    match risk {
        RiskLevel::Safe => format!("[{}]", "SAFE".green()),
        RiskLevel::Warning => format!("[{}]", "WARNING".yellow()),
        RiskLevel::Danger => format!("[{}]", "DANGER".red()),
    }
}

fn truncate_address(addr: &str) -> String {
    if addr.len() > 12 {
        format!("{}...{}", &addr[..6], &addr[addr.len() - 4..])
    } else {
        addr.to_string()
    }
}
