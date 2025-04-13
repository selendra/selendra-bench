# Selendra Network Benchmarking Tool

A comprehensive benchmarking tool for measuring and analyzing the performance of the Selendra Network. This tool provides detailed metrics about transaction throughput, inclusion times, finality times, and network resource usage.

## Features

- Multiple transaction type support (Transfer, ERC20 Transfer, Complex Contract)
- Real-time metrics collection
- Configurable benchmarking parameters
- Detailed performance statistics
- Support for real transactions with customizable seed phrases
- JSON output for results

## Build Instructions

### Prerequisites

- Rust and Cargo (latest stable version recommended)
- Git

### Building from Source

1. Clone the repository:
   ```bash
   git clone https://github.com/selendra/selendra-bench.git
   cd selendra-bench
   ```

2. Build the project:
   ```bash
   # Build in debug mode
   cargo build
   
   # Build in release mode (recommended for benchmarking)
   cargo build --release
   ```

The compiled binary will be available at:
- Debug mode: `./target/debug/selendra-bench`
- Release mode: `./target/release/selendra-bench`

## Architecture

The tool is structured into several modules:

### Core Modules

- `types`: Defines the data structures used throughout the application
- `client`: Handles communication with the Selendra Network node
- `benchmark`: Implements the benchmarking logic
- `metrics`: Collects and processes network metrics

### Data Structures

#### BenchmarkStats

```rust
pub struct BenchmarkStats {
    pub submitted: usize,
    pub successful: usize,
    pub failed: usize,
    pub inclusion_times: Vec<Duration>,
    pub finality_times: Vec<Duration>,
    pub block_heights: HashMap<u32, usize>,
    pub block_sizes: HashMap<u32, usize>,
    pub tx_queue_depth: VecDeque<(Instant, usize)>,
    pub errors: HashMap<String, usize>,
    pub node_metrics: Vec<NodeMetrics>,
}
```

#### NodeMetrics

```rust
pub struct NodeMetrics {
    pub timestamp: Instant,
    pub peer_count: usize,
    pub transaction_pool_size: usize,
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
    pub network_tx_bytes: u64,
    pub network_rx_bytes: u64,
    pub block_height: u32,
    pub block_size_bytes: usize,
    pub time_to_finality_ms: u64,
}
```

## Usage

### Command Line Arguments

```bash
selendra-bench --node-url <URL> [OPTIONS]
```

#### Required Arguments
- `--node-url <URL>`: WebSocket URL of the Selendra node

#### General Options
- `--accounts <NUM>`: Number of accounts to use for benchmarking (default: 100)
- `--tx-type <TYPE>`: Transaction type (transfer, erc20, complex) (default: transfer)
- `--tps-target <TPS>`: Target transactions per second (default: 100)
- `--duration <SECONDS>`: Benchmark duration in seconds (default: 60)
- `--output <FILE>`: Output file for detailed benchmark results (JSON) (optional)

#### Real Transaction Options
- `--real-transactions`: Use real transactions instead of simulated ones
- `--seed-phrase <PHRASE>`: Seed phrase for the account to use for real transactions
- `--min-amount <AMOUNT>`: Minimum amount to use for real transactions (default: 100000)
- `--max-amount <AMOUNT>`: Maximum amount to use for real transactions (default: 200000)

### Running the Benchmark Script

The project includes a convenient benchmarking script that runs multiple tests at different TPS levels:

```bash
# Make the script executable
chmod +x benchmark-blocksize.sh

# Run simulated benchmarks (no real transactions)
./benchmark-blocksize.sh --node-url <WEBSOCKET_URL> --duration <SECONDS> --tps <TPS_LEVELS>

# Example: Run simulated benchmarks for 30 seconds at 1, 5, 10, 20, and 50 TPS
./benchmark-blocksize.sh --node-url wss://rpcx.selendra.org --duration 30 --tps 1,5,10,20,50

# Run with real transactions
./benchmark-blocksize.sh --node-url <WEBSOCKET_URL> --use-real-transactions --seed-phrase "your seed phrase" --duration <SECONDS> --tps <TPS_LEVELS>

# Example: Run real transaction benchmarks for 10 seconds at 1, 2, and 3 TPS
./benchmark-blocksize.sh --node-url wss://rpcx.selendra.org --use-real-transactions --seed-phrase "your seed phrase" --duration 10 --tps 1,2,3
```

> ⚠️ **Important**: When using `--real-transactions` with a seed phrase, ensure you're using a testing/development account with minimal funds. Real transactions will transfer actual tokens from your account!

### Basic Example

```bash
# Run a single benchmark with simulated transactions
./target/release/selendra-bench --node-url wss://rpcx.selendra.org --accounts 50 --tx-type transfer --tps-target 10 --duration 30 --output results.json

# Run a single benchmark with real transactions
./target/release/selendra-bench --node-url wss://rpcx.selendra.org --accounts 5 --tx-type transfer --tps-target 1 --duration 10 --real-transactions --seed-phrase "your seed phrase" --min-amount 1000000 --max-amount 5000000 --output real_results.json
```

## Implementation Details

### Node Client

The `NodeClient` struct provides methods for interacting with the Selendra Network:

```rust
pub struct NodeClient {
    client: Arc<WsClient>,
    // Additional fields for real transaction support
}

impl NodeClient {
    pub async fn submit_transaction(&self, account: &Account, tx_type: &TxType, ...) -> Result<(), BenchError>;
    pub async fn submit_real_transfer(&self, ...) -> Result<(), BenchError>;
    pub async fn submit_simulated_transaction(&self, ...) -> Result<(), BenchError>;
    pub async fn get_chain_metadata(&self) -> Result<ChainMetadata, BenchError>;
}
```

### Benchmark Runner

The `BenchmarkRunner` struct orchestrates the benchmarking process:

```rust
pub struct BenchmarkRunner {
    node_client: NodeClient,
    accounts: Vec<Account>,
    tx_type: TxType,
    target_tps: usize,
    duration: u64,
    use_real_transactions: bool,
}

impl BenchmarkRunner {
    pub async fn new(
        node_url: &str,
        num_accounts: usize,
        tx_type: TxType,
        target_tps: usize,
        duration: u64,
        use_real_transactions: bool,
        seed_phrase: Option<String>,
        min_amount: u128,
        max_amount: u128,
    ) -> Result<Self, Box<dyn Error + Send + Sync>>;
    
    pub async fn run(&self) -> Result<BenchmarkStats, Box<dyn Error + Send + Sync>>;
}
```

## Output Format

The benchmark results are saved in JSON format with statistics about:

- Number of transactions submitted
- Success and failure rates
- Transaction inclusion and finality times
- Block sizes and heights
- Error distribution
- Network metrics

## Future Improvements

1. Add support for more transaction types
2. Implement distributed benchmarking
3. Add visualization capabilities
4. Enhance error handling and reporting
5. Add support for custom benchmarking scenarios

## License

This project is licensed under the MIT License - see the LICENSE file for details. 