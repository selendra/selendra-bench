mod types;
mod client;
mod benchmark;
mod metrics;

use clap::{Command, Arg};
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::time::Duration;

use crate::{
    benchmark::BenchmarkRunner,
    types::{BenchmarkStats, TxType},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let matches = Command::new("selendra-bench")
        .version("0.2.0")
        .author("Selendra Network")
        .about("Enhanced benchmarking tool for Selendra Network")
        .arg(
            Arg::new("node-url")
                .long("node-url")
                .value_name("URL")
                .help("WebSocket URL of the Selendra node")
                .required(true),
        )
        .arg(
            Arg::new("accounts")
                .long("accounts")
                .value_name("NUM")
                .help("Number of accounts to use for benchmarking")
                .default_value("100"),
        )
        .arg(
            Arg::new("tx-type")
                .long("tx-type")
                .value_name("TYPE")
                .help("Transaction type: transfer, erc20, complex")
                .default_value("transfer"),
        )
        .arg(
            Arg::new("tps-target")
                .long("tps-target")
                .value_name("TPS")
                .help("Target transactions per second")
                .default_value("100"),
        )
        .arg(
            Arg::new("duration")
                .long("duration")
                .value_name("SECONDS")
                .help("Benchmark duration in seconds")
                .default_value("60"),
        )
        .arg(
            Arg::new("output")
                .long("output")
                .value_name("FILE")
                .help("Output file for detailed benchmark results (JSON)")
                .required(false),
        )
        .arg(
            Arg::new("real-transactions")
                .long("real-transactions")
                .help("Use real transactions instead of simulated ones")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("seed-phrase")
                .long("seed-phrase")
                .value_name("PHRASE")
                .help("Seed phrase for the account to use for real transactions")
                .required(false),
        )
        .arg(
            Arg::new("min-amount")
                .long("min-amount")
                .value_name("AMOUNT")
                .help("Minimum amount to use for real transactions (in smallest unit)")
                .default_value("100000"),
        )
        .arg(
            Arg::new("max-amount")
                .long("max-amount")
                .value_name("AMOUNT")
                .help("Maximum amount to use for real transactions (in smallest unit)")
                .default_value("200000"),
        )
        .get_matches();

    let node_url = matches.get_one::<String>("node-url").unwrap();
    let num_accounts = matches
        .get_one::<String>("accounts")
        .unwrap()
        .parse::<usize>()?;
    let tx_type = matches
        .get_one::<String>("tx-type")
        .unwrap()
        .parse::<TxType>()?;
    let target_tps = matches
        .get_one::<String>("tps-target")
        .unwrap()
        .parse::<usize>()?;
    let duration = matches
        .get_one::<String>("duration")
        .unwrap()
        .parse::<u64>()?;
    let output_file = matches.get_one::<String>("output");
    let use_real_transactions = matches.get_flag("real-transactions");
    let seed_phrase = matches.get_one::<String>("seed-phrase").cloned();
    let min_amount = matches
        .get_one::<String>("min-amount")
        .unwrap()
        .parse::<u128>()?;
    let max_amount = matches
        .get_one::<String>("max-amount")
        .unwrap()
        .parse::<u128>()?;

    // Validate parameters for real transactions
    if use_real_transactions && seed_phrase.is_none() {
        return Err("Seed phrase is required for real transactions".into());
    }

    if min_amount > max_amount {
        return Err("Minimum amount cannot be greater than maximum amount".into());
    }

    println!("Selendra Network Enhanced Benchmarking Tool v0.2.0");
    println!("=================================================");
    println!("Node URL: {}", node_url);
    println!("Number of accounts: {}", num_accounts);
    println!("Transaction type: {:?}", tx_type);
    println!("Target TPS: {}", target_tps);
    println!("Duration: {} seconds", duration);
    
    if use_real_transactions {
        println!("Using REAL transactions");
        println!("Amount range: {} to {}", min_amount, max_amount);
    } else {
        println!("Using simulated transactions");
    }
    
    println!();

    // Create and run benchmark
    let runner = BenchmarkRunner::new(
        node_url, 
        num_accounts, 
        tx_type, 
        target_tps, 
        duration,
        use_real_transactions,
        seed_phrase,
        min_amount,
        max_amount
    ).await?;
    
    let stats = runner.run().await?;

    // Print results
    print_results(&stats);

    // Save results to file if specified
    if let Some(output_path) = output_file {
        save_results(&stats, output_path)?;
    } else {
        // Print results as JSON to console if no output file is specified
        let json = serde_json::to_string_pretty(&stats)?;
        println!("\nJSON Results:\n{}", json);
    }

    Ok(())
}

fn print_results(stats: &BenchmarkStats) {
    println!("\nBenchmark Results:");
    println!("=================");
    println!("Total Transactions: {}", stats.submitted);
    println!("Successful: {}", stats.successful);
    println!("Failed: {}", stats.failed);
    println!("Success Rate: {:.2}%", 
        (stats.successful as f64 / stats.submitted as f64) * 100.0);

    if !stats.inclusion_times.is_empty() {
        let avg_inclusion = stats.inclusion_times.iter()
            .sum::<Duration>() / stats.inclusion_times.len() as u32;
        println!("Average Inclusion Time: {:?}", avg_inclusion);
    }

    if !stats.finality_times.is_empty() {
        let avg_finality = stats.finality_times.iter()
            .sum::<Duration>() / stats.finality_times.len() as u32;
        println!("Average Finality Time: {:?}", avg_finality);
    }

    // Print error statistics if any
    if !stats.errors.is_empty() {
        println!("\nError Statistics:");
        for (error, count) in &stats.errors {
            println!("{}: {}", error, count);
        }
    }
}

fn save_results(stats: &BenchmarkStats, path: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let json = serde_json::to_string_pretty(stats)?;
    let mut file = File::create(path)?;
    file.write_all(json.as_bytes())?;
    println!("\nResults saved to: {}", path);
    Ok(())
} 