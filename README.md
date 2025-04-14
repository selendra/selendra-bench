# Selendra Network Benchmarking Tool

An enhanced benchmarking tool for testing and analyzing the performance of the Selendra Network.

## Features

- **Transaction Benchmarking**: Test network performance with various transaction types
- **Transaction Per Second (TPS) Testing**: Determine maximum network capacity
- **Connection Failover**: Automatic switching between primary and backup nodes
- **Transaction Status Tracking**: Monitor transaction confirmation and finalization
- **Detailed Metrics Collection**: Gather comprehensive performance data

## Installation

### Prerequisites

- Rust and Cargo (latest stable version)
- Node.js (for real transaction scripts)
- Basic command-line tools: bash, bc, etc.

### Building

Clone the repository and build the project:

```bash
git clone https://github.com/your-username/selendra-bench.git
cd selendra-bench
cargo build --release
```

## Usage

### Basic Benchmark

Run a simple benchmark with simulated transactions:

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

The project includes a script for determining the maximum TPS capacity of the network. See [TPS Testing README](TPS_TEST_README.md) for details.

Basic usage:

```bash
./test_max_tps.sh --seed-phrase "your seed phrase"
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

## Output and Results

The benchmark tool outputs several types of data:

- **Console Output**: Real-time status updates and summary statistics
- **JSON Output**: Detailed metrics in structured format (when using --output)
- **Transaction Status**: Confirmation status and block numbers
- **Performance Metrics**: Success rates, latency measurements, and more

## Project Structure

- `src/`: Source code for the benchmark tool
  - `src/benchmark/`: Benchmarking logic
  - `src/client/`: Node client implementation
  - `src/types/`: Data structures
  - `src/main.rs`: CLI entry point
- `test_max_tps.sh`: Script for testing maximum TPS capacity

## Troubleshooting

- **Connection Issues**: If you experience connection problems, try using the backup URL feature
- **Transaction Errors**: Ensure your account has sufficient funds for all transactions
- **Performance Problems**: Try reducing the TPS target or increasing the number of accounts
- **Script Errors**: Make sure Node.js is installed for real transaction scripts

## License

[MIT License](LICENSE) 