use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

// Constants
pub const BENCHMARK_SEED: u64 = 0x1234_5678_9ABC_DEF0;
pub const DEFAULT_BALANCE: u128 = 1_000_000_000_000;
pub const MAX_CONCURRENT_REQUESTS: usize = 100;
pub const HISTORY_WINDOW: usize = 20;

#[derive(Default, Clone, Serialize, Debug)]
pub struct BenchmarkStats {
    pub submitted: usize,
    pub successful: usize,
    pub failed: usize,
    #[serde(skip)]
    pub inclusion_times: Vec<Duration>,
    #[serde(skip)]
    pub finality_times: Vec<Duration>,
    pub block_heights: HashMap<u32, usize>,
    pub block_sizes: HashMap<u32, usize>,
    #[serde(skip)]
    pub tx_queue_depth: VecDeque<(Instant, usize)>,
    pub errors: HashMap<String, usize>,
    #[serde(skip)]
    pub node_metrics: Vec<NodeMetrics>,
}

#[derive(Clone, Debug)]
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

impl Default for NodeMetrics {
    fn default() -> Self {
        Self {
            timestamp: Instant::now(),
            peer_count: 0,
            transaction_pool_size: 0,
            memory_usage_mb: 0.0,
            cpu_usage_percent: 0.0,
            network_tx_bytes: 0,
            network_rx_bytes: 0,
            block_height: 0,
            block_size_bytes: 0,
            time_to_finality_ms: 0,
        }
    }
}

#[derive(Clone)]
pub struct Account {
    pub address: String,
    pub private_key: Vec<u8>,
    pub nonce: Arc<Mutex<u32>>,
}

#[derive(Clone, Debug)]
pub enum TxType {
    Transfer,
    Erc20Transfer,
    ComplexContract,
}

impl std::str::FromStr for TxType {
    type Err = Box<dyn std::error::Error + Send + Sync>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "transfer" => Ok(TxType::Transfer),
            "erc20" => Ok(TxType::Erc20Transfer),
            "complex" => Ok(TxType::ComplexContract),
            _ => Err(format!("Unknown transaction type: {}", s).into()),
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct ChainMetadata {
    pub genesis_hash: String,
    pub runtime_version: u32,
    pub tx_version: u32,
    pub ss58_format: u8,
    pub token_decimals: u8,
    pub token_symbol: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransactionStatus {
    pub tx_hash: String,
    pub block_number: Option<u32>,
    pub finalized: bool,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BlockInfo {
    pub number: u32,
    pub hash: String,
    pub parent_hash: String,
    pub state_root: String,
    pub extrinsics_root: String,
    pub extrinsics: Vec<String>,
    pub size_bytes: usize,
    pub timestamp: Option<u64>,
} 