# Selendra Network Benchmarking Tool

A benchmarking tool for testing and analyzing the performance of the Selendra Network.

## Installation

### Prerequisites

- Rust and Cargo (latest stable version)
- Node.js and npm (for real transaction scripts)

### Building

```bash
git clone https://github.com/selendra/selendra-bench.git
cd selendra-bench
cargo build --release
```

## Quick Start Commands

### Simple Benchmark

```bash
./target/release/selendra-bench --node-url wss://rpcx.selendra.org \
  --accounts 50 --tx-type transfer --tps-target 10 --duration 60
```

### Testing TPS (Transactions Per Second)

```bash
# Test with moderate TPS
cargo run --release -- --node-url wss://rpcx.selendra.org --tx-type transfer \
  --tps-target 50 --duration 300 --accounts 200 --output tps_test_50.json

# Test with higher TPS
cargo run --release -- --node-url wss://rpcx.selendra.org --tx-type transfer \
  --tps-target 100 --duration 300 --accounts 200 --output tps_test_100.json
```

### Testing Block Size

```bash
# Simple transfers (smallest block size)
cargo run --release -- --node-url wss://rpcx.selendra.org --tx-type transfer \
  --tps-target 100 --duration 300 --output blocksize_transfer.json

# ERC20 transfers (medium block size)
cargo run --release -- --node-url wss://rpcx.selendra.org --tx-type erc20 \
  --tps-target 100 --duration 300 --output blocksize_erc20.json

# Complex contract calls (largest block size)
cargo run --release -- --node-url wss://rpcx.selendra.org --tx-type complex \
  --tps-target 100 --duration 300 --output blocksize_complex.json
```

### Real Transaction Benchmark

```bash
./target/release/selendra-bench --node-url wss://rpcx.selendra.org \
  --accounts 5 --tx-type transfer --tps-target 1 --duration 300 \
  --real-transactions --seed-phrase "your seed phrase" \
  --min-amount 1000000 --max-amount 5000000
```

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
- `--min-amount <AMOUNT>`: Minimum amount for transactions (default: 100000)
- `--max-amount <AMOUNT>`: Maximum amount for transactions (default: 200000)

## Troubleshooting

- **Connection Issues**: Try using the backup URL feature
- **Transaction Errors**: Ensure your account has sufficient funds
- **Performance Problems**: Try reducing the TPS target or increasing the number of accounts
- **Script Errors**: Make sure Node.js is installed for real transaction scripts

## Project Structure

- `src/benchmark/`: Benchmarking logic
- `src/client/`: Node client implementation
- `src/types/`: Data structures
- `src/main.rs`: CLI entry point

## License

[MIT License](LICENSE) 