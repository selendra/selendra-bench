# Selendra Network TPS Testing Tool

This tool helps determine the maximum transactions per second (TPS) capacity of the Selendra Network by running incremental tests.

## Overview

The TPS Testing Tool systematically tests the network's performance by:

1. Starting with a low TPS value
2. Incrementally increasing the TPS with each test
3. Monitoring transaction success rates
4. Identifying the point where performance deteriorates

## Requirements

- Bash shell
- `bc` command (basic calculator)
- Compiled `selendra-bench` binary in the `target/release` directory
- A valid Selendra account with sufficient funds

## Installation

1. Ensure you've built the `selendra-bench` tool:
   ```
   cargo build --release
   ```

2. Make the testing script executable:
   ```
   chmod +x test_max_tps.sh
   ```

## Usage

Basic usage:

```bash
./test_max_tps.sh --seed-phrase "your seed phrase"
```

Full options:

```bash
./test_max_tps.sh --seed-phrase "your seed phrase" [OPTIONS]
```

### Required Arguments

- `--seed-phrase "PHRASE"`: Your account's seed phrase (required)

### Optional Arguments

- `--node-url URL`: Primary node URL (default: wss://rpcx.selendra.org)
- `--backup-url URL`: Backup node URL (default: wss://rpc.selendra.org)
- `--duration SECONDS`: Duration of each test in seconds (default: 120)
- `--min-amount AMOUNT`: Minimum transaction amount (default: 1000000)
- `--max-amount AMOUNT`: Maximum transaction amount (default: 5000000)
- `--accounts NUMBER`: Number of accounts to use (default: 5)
- `--output-dir DIRECTORY`: Output directory (default: tps_test_results)
- `--start-tps NUMBER`: Starting TPS value (default: 1)
- `--max-tps NUMBER`: Maximum TPS to test (default: 10)
- `--step NUMBER`: TPS increment step (default: 1)
- `--help`: Display help message

## Examples

Basic test with default parameters:
```bash
./test_max_tps.sh --seed-phrase "your seed phrase"
```

Custom test with higher TPS range:
```bash
./test_max_tps.sh --seed-phrase "your seed phrase" \
  --start-tps 5 --max-tps 30 --step 5 \
  --duration 180
```

Testing with a specific node:
```bash
./test_max_tps.sh --seed-phrase "your seed phrase" \
  --node-url "wss://your-custom-node.example.com" \
  --backup-url "wss://backup-node.example.com"
```

## Output and Results

The script creates an output directory (default: `tps_test_results`) containing:

- `config.txt`: Test configuration and results for each TPS level
- `results.csv`: CSV data with columns: tps, submitted, confirmed, failed, success_rate
- `summary.txt`: Simple summary showing TPS and success rates
- `tps_X.json`: Detailed output file for each TPS test

## Interpreting Results

The tool automatically stops testing if the success rate drops below 50%, which generally indicates network capacity has been reached.

For most production applications:
- Choose a TPS level with >90% success rate for reliable operation
- The maximum theoretical TPS is where success rate begins to decline significantly
- Brief summary is displayed at the end of the test

## Tips for Accurate Testing

1. **Ensure sufficient funds**: Your account needs enough funds to cover all transactions
2. **Run multiple tests**: Network conditions vary; run tests at different times
3. **Longer duration for verification**: Use longer durations (e.g., `--duration 300`) for verification tests
4. **Allow network recovery**: The script waits 30 seconds between tests to avoid skewed results
5. **Test with different node URLs**: Results can vary based on the node being tested

## Troubleshooting

- If you see "Error: Seed phrase is required", you must provide a valid seed phrase
- If JSON files aren't created, check your network connection to the node
- If the success rate is consistently low, your account may need more funds
- If tests end prematurely, check the node's status and log files 