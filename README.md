# Selendra Network Benchmarking Tool

An enhanced benchmarking tool for testing and analyzing the performance of the Selendra Network.

## Table of Contents

- [Features](#features)
- [Installation](#installation)
- [Basic Usage](#basic-usage)
- [TPS Testing](#tps-testing)
- [Block Size Benchmarking](#block-size-benchmarking)
- [Command Line Options](#command-line-options)
- [Output and Results](#output-and-results)
- [Logging](#logging)
- [Project Structure](#project-structure)
- [Troubleshooting](#troubleshooting)
- [License](#license)

## Features

- **Transaction Benchmarking**: Test network performance with various transaction types
- **Transaction Per Second (TPS) Testing**: Determine maximum network capacity
- **Block Size Optimization**: Benchmark performance with different block sizes
- **Connection Failover**: Automatic switching between primary and backup nodes
- **Transaction Status Tracking**: Monitor transaction confirmation and finalization
- **Detailed Metrics Collection**: Gather comprehensive performance data

## Installation

### Prerequisites

- Rust and Cargo (latest stable version)
- Node.js and npm (for real transaction scripts)
- Basic command-line tools: bash, bc, jq, etc.

### Building

Clone the repository and build the project:

```bash
git clone https://github.com/your-username/selendra-bench.git
cd selendra-bench
cargo build --release
```

## Basic Usage

### Simple Benchmark

Run a basic benchmark with simulated transactions:

```bash
./target/release/selendra-bench --node-url wss://rpcx.selendra.org \
  --accounts 50 --tx-type transfer --tps-target 10 --duration 60
```

### Real Transaction Benchmark

For benchmarking with actual transactions (requires a funded account):

```bash
./target/release/selendra-bench --node-url wss://rpcx.selendra.org \
  --accounts 5 --tx-type transfer --tps-target 1 --duration 300 \
  --real-transactions --seed-phrase "your seed phrase" \
  --min-amount 1000000 --max-amount 5000000
```

### Long-Running Benchmark (with backup URL)

For extended testing with automatic failover:

```bash
./target/release/selendra-bench --node-url wss://rpcx.selendra.org \
  --backup-url wss://rpc.selendra.org --accounts 5 --tx-type transfer \
  --tps-target 1 --duration 25200 --real-transactions \
  --seed-phrase "your seed phrase" --min-amount 1000000 --max-amount 5000000 \
  --output results.json
```

## TPS Testing

The TPS Testing Tool helps determine the maximum transactions per second capacity of the Selendra Network by systematically testing with incremental loads.

### Overview

The tool works by:

1. Starting with a low TPS value
2. Incrementally increasing the TPS with each test
3. Monitoring transaction success rates
4. Identifying the point where performance deteriorates

### TPS Test Usage

Basic usage:

```bash
./src/scripts/test_max_tps.sh --seed-phrase "your seed phrase"
```

Full options:

```bash
./src/scripts/test_max_tps.sh --seed-phrase "your seed phrase" [OPTIONS]
```

### TPS Test Required Arguments

- `--seed-phrase "PHRASE"`: Your account's seed phrase (required)

### TPS Test Optional Arguments

- `--node-url URL`: Primary node URL (default: wss://rpcx.selendra.org)
- `--backup-url URL`: Backup node URL (default: wss://rpc.selendra.org)
- `--duration SECONDS`: Duration of each test in seconds (default: 120)
- `--min-amount AMOUNT`: Minimum transaction amount in smallest units (default: 1000000)
- `--max-amount AMOUNT`: Maximum transaction amount in smallest units (default: 5000000)
- `--accounts NUMBER`: Number of accounts to use (default: 5)
- `--output-dir DIRECTORY`: Output directory (default: tps_test_results)
- `--start-tps NUMBER`: Starting TPS value (default: 1)
- `--max-tps NUMBER`: Maximum TPS to test (default: 10)
- `--step NUMBER`: TPS increment step (default: 1)
- `--help`: Display help message

### TPS Test Examples

Basic test with default parameters:
```bash
./src/scripts/test_max_tps.sh --seed-phrase "your seed phrase"
```

Custom test with higher TPS range:
```bash
./src/scripts/test_max_tps.sh --seed-phrase "your seed phrase" \
  --start-tps 5 --max-tps 30 --step 5 \
  --duration 180
```

### TPS Test Output

The script creates an output directory containing:

- `config.txt`: Test configuration and results for each TPS level
- `results.csv`: CSV data with columns: tps, submitted, confirmed, failed, success_rate
- `summary.txt`: Simple summary showing TPS and success rates
- `tps_X.json`: Detailed output file for each TPS test

### Interpreting TPS Test Results

- The tool stops testing if the success rate drops below 50%
- For most applications, choose a TPS level with >90% success rate
- Maximum theoretical TPS is where success rate begins to decline

## Block Size Benchmarking

The Block Size Benchmarking tool tests Selendra Network performance across different TPS levels, with a focus on determining optimal block size parameters.

### Block Size Benchmark Usage

Basic usage:

```bash
./src/scripts/benchmark-blocksize.sh --node-url wss://rpcx.selendra.org \
  --seed-phrase "your seed phrase" --use-real-transactions
```

### Block Size Benchmark Parameters

- `--node-url`: The WebSocket URL of the node to test (default: wss://rpcx.selendra.org)
- `--duration`: Duration of each test in seconds (default: 10)
- `--output-dir`: Directory for results (default: benchmark_results/timestamp)
- `--tps`: Comma-separated list of TPS levels to test (default: 1,2,3,5,8)
- `--tx-type`: Transaction type: transfer, erc20, complex (default: transfer)
- `--accounts`: Number of accounts to use (default: 5)
- `--use-real-transactions`: Flag to enable real transactions
- `--seed-phrase`: Required if using real transactions
- `--min-amount`: Minimum transaction amount in smallest units (default: 1000000)
- `--max-amount`: Maximum transaction amount in smallest units (default: 5000000)

### Block Size Benchmark Output

The script generates:

- `benchmark_config.txt`: Configuration details
- `benchmark_[tx_type]_[tps]tps.json`: Raw benchmark results for each TPS level
- `summary.txt`: Summary of results showing submitted/successful/failed transactions
- `recommendations.txt`: Automated TPS recommendations based on success rates

### Block Size Benchmark Recommendations

The benchmark results provide:

1. **Optimal TPS**: The highest TPS with >95% success rate
2. **Maximum Safe TPS**: The highest TPS with >80% success rate

For production environments, it's recommended to use the Optimal TPS value for consistent performance.

## Command Line Options

### Required Arguments

- `--node-url <URL>`: WebSocket URL of the Selendra node

### General Options

- `--backup-url <URL>`: Backup WebSocket URL for failover
- `--accounts <NUM>`: Number of accounts to use (default: 100)
- `--tx-type <TYPE>`: Transaction type: transfer, erc20, complex (default: transfer)
- `--tps-target <TPS>`: Target transactions per second (default: 100)
- `--duration <SECONDS>`: Benchmark duration in seconds (default: 60)
- `--output <FILE>`: Output file for detailed benchmark results in JSON format

### Real Transaction Options

- `--real-transactions`: Use real transactions instead of simulated ones
- `--seed-phrase <PHRASE>`: Seed phrase for the account (required for real transactions)
- `--min-amount <AMOUNT>`: Minimum amount for transactions in smallest units (default: 100000)
- `--max-amount <AMOUNT>`: Maximum amount for transactions in smallest units (default: 200000)

## Output and Results

The benchmark tool outputs several types of data:

- **Console Output**: Real-time status updates and summary statistics
- **JSON Output**: Detailed metrics in structured format (when using --output)
- **Transaction Status**: Confirmation status and block numbers
- **Performance Metrics**: Success rates, latency measurements, and more

## Logging

The benchmarking tool produces detailed logs that are stored in the `logs/` directory. Log files include:

- Transaction submissions and confirmations
- Error messages and warnings
- Performance metrics and timing data
- Node connection status

You can modify the logging verbosity by setting the `RUST_LOG` environment variable:

```bash
# For detailed logging
RUST_LOG=debug ./target/release/selendra-bench [options]

# For minimal logging
RUST_LOG=warn ./target/release/selendra-bench [options]
```

## Project Structure

- `src/`: Source code for the benchmark tool
  - `src/benchmark/`: Benchmarking logic
  - `src/client/`: Node client implementation
  - `src/types/`: Data structures
  - `src/main.rs`: CLI entry point
  - `src/scripts/`: Shell scripts for benchmarking
    - `src/scripts/test_max_tps.sh`: Script for testing maximum TPS capacity
    - `src/scripts/benchmark-blocksize.sh`: Script for block size optimization benchmarks

## Troubleshooting

### Common Issues

- **Connection Issues**: If you experience connection problems, try using the backup URL feature
- **Transaction Errors**: Ensure your account has sufficient funds for all transactions
- **Performance Problems**: Try reducing the TPS target or increasing the number of accounts
- **Script Errors**: Make sure Node.js is installed for real transaction scripts

### TPS Testing Issues

- If you see "Error: Seed phrase is required", you must provide a valid seed phrase
- If JSON files aren't created, check your network connection to the node
- If the success rate is consistently low, your account may need more funds
- If tests end prematurely, check the node's status and log files

### Block Size Benchmark Issues

- For real transactions, seed phrase must be provided
- If benchmarks fail, check the output for error messages and logs in the `logs/` directory
- After running with real transactions, wait for pending transactions to settle before running again

## License

[MIT License](LICENSE) 