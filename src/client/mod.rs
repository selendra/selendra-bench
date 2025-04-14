use crate::types::{Account, ChainMetadata};
use jsonrpsee::{
    core::client::ClientT,
    rpc_params,
    ws_client::{WsClient, WsClientBuilder},
};
use rand::{thread_rng, Rng};
use std::{error::Error, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use hex;
use serde_json;
use serde_json::json;
use log::{debug, error, info, warn};
use tokio::time::{sleep};

// Add the transaction module
pub mod transaction;

// Custom error type that can be sent between threads
#[derive(Debug)]
pub enum BenchError {
    ConnectionError(String),
    RPCError(String),
    SerializationError(String),
    CommandError(String),
    ParseError(String),
    IOError(String),
    TimeoutError(String),
    NonceError(String),
}

impl std::fmt::Display for BenchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for BenchError {}

impl From<String> for BenchError {
    fn from(s: String) -> Self {
        BenchError(s)
    }
}

impl From<&str> for BenchError {
    fn from(s: &str) -> Self {
        BenchError(s.to_string())
    }
}

// Implement specific From instances for error types we need
impl From<jsonrpsee::core::Error> for BenchError {
    fn from(e: jsonrpsee::core::Error) -> Self {
        BenchError(e.to_string())
    }
}

impl From<Box<dyn Error + Send + Sync>> for BenchError {
    fn from(e: Box<dyn Error + Send + Sync>) -> Self {
        BenchError(e.to_string())
    }
}

const MAX_RECONNECT_ATTEMPTS: u32 = 5;
const RECONNECT_DELAY_MS: u64 = 1000;
const TX_STATUS_CHECK_INTERVAL_MS: u64 = 2000;
const TX_STATUS_TIMEOUT_SECONDS: u64 = 120;

#[derive(Clone)]
pub struct NodeClient {
    pub client: Arc<WsClient>,
    pub endpoint: String,
    pub nonce: Arc<Mutex<u32>>,
    primary_url: String,
    backup_urls: Vec<String>,
    tx_script_dir: String,
    use_real_transactions: bool,
    seed_phrase: Option<String>,
    min_amount: u128,
    max_amount: u128,
}

impl NodeClient {
    pub async fn new(url: &str, use_real_transactions: bool, seed_phrase: Option<String>, tx_script_dir: &str, min_amount: u128, max_amount: u128) -> Result<Self, BenchError> {
        // Parse the URL, it might contain multiple comma-separated URLs
        let urls: Vec<String> = url.split(',').map(|s| s.trim().to_string()).collect();
        
        // Set up the primary and backup URLs
        let primary_url = urls.first().unwrap_or(&"wss://rpcx.selendra.org".to_string()).clone();
        let mut backup_urls = Vec::new();
        
        // Add explicitly provided backup URLs (from the comma-separated list)
        for i in 1..urls.len() {
            backup_urls.push(urls[i].clone());
        }
        
        // Add default backup URLs if needed
        if backup_urls.is_empty() {
            // Create backup URLs - we'll add the selendra.org RPCs
            if primary_url == "wss://rpcx.selendra.org" {
                backup_urls.push("wss://rpc.selendra.org".to_string());
            } else if primary_url == "wss://rpc.selendra.org" {
                backup_urls.push("wss://rpcx.selendra.org".to_string());
            } else {
                // If not using one of the standard Selendra RPCs, add both as backups
                if !primary_url.contains("rpcx.selendra.org") {
                    backup_urls.push("wss://rpcx.selendra.org".to_string());
                }
                if !primary_url.contains("rpc.selendra.org") {
                    backup_urls.push("wss://rpc.selendra.org".to_string());
                }
            }
        }
        
        println!("Primary RPC URL: {}", primary_url);
        if !backup_urls.is_empty() {
            println!("Backup RPC URLs: {}", backup_urls.join(", "));
        }
        
        // Connect to the primary URL
        let client = match WsClientBuilder::default()
            .build(&primary_url)
            .await {
                Ok(client) => client,
                Err(e) => {
                    println!("Failed to connect to primary URL {}: {}", primary_url, e);
                    
                    // Try to connect to each backup URL
                    let mut connected = false;
                    let mut client = None;
                    
                    for backup_url in &backup_urls {
                        println!("Trying backup URL: {}", backup_url);
                        match WsClientBuilder::default().build(backup_url).await {
                            Ok(c) => {
                                println!("Successfully connected to backup URL: {}", backup_url);
                                client = Some(c);
                                connected = true;
                                break;
                            },
                            Err(e) => {
                                println!("Failed to connect to backup URL {}: {}", backup_url, e);
                            }
                        }
                    }
                    
                    if !connected {
                        return Err(format!("Failed to connect to any RPC endpoint. Primary URL: {}, Error: {}", primary_url, e).into());
                    }
                    
                    client.unwrap()
                }
            };
        
        // Create an absolute path for tx_script_dir
        let current_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let absolute_script_dir = if std::path::Path::new(tx_script_dir).is_absolute() {
            tx_script_dir.to_string()
        } else {
            current_dir.join(tx_script_dir).to_string_lossy().to_string()
        };
        
        println!("Transaction script directory: {}", absolute_script_dir);
        
        // If using real transactions, set up the transaction script
        if use_real_transactions {
            transaction::create_transaction_script(&absolute_script_dir)?;
        }
        
        Ok(Self {
            client: Arc::new(client),
            endpoint: primary_url,
            nonce: Arc::new(Mutex::new(0)),
            primary_url,
            backup_urls,
            tx_script_dir: absolute_script_dir,
            use_real_transactions,
            seed_phrase,
            min_amount,
            max_amount,
        })
    }
    
    /// Attempts to reconnect to the node 
    pub async fn reconnect(&self) -> Result<(), BenchError> {
        info!("Attempting to reconnect to node at {}", self.endpoint);
        
        for attempt in 1..=MAX_RECONNECT_ATTEMPTS {
            // Try to close existing connection gracefully
            match Arc::get_mut(&mut self.client) {
                Some(client) => {
                    // Ignore errors during close, as connection might already be broken
                    let _ = client.close().await;
                },
                None => {
                    // Can't get exclusive access to client, likely due to other references
                    // Create a new Arc instead
                    debug!("Unable to get exclusive access to client for closing, creating new client");
                }
            }
            
            // Attempt to create a new connection
            match WsClientBuilder::default().build(&self.endpoint).await {
                Ok(new_client) => {
                    self.client = Arc::new(new_client);
                    info!("Successfully reconnected to node");
                    return Ok(());
                },
                Err(e) => {
                    warn!("Reconnection attempt {}/{} failed: {}", 
                         attempt, MAX_RECONNECT_ATTEMPTS, e);
                    
                    if attempt < MAX_RECONNECT_ATTEMPTS {
                        sleep(Duration::from_millis(RECONNECT_DELAY_MS)).await;
                    }
                }
            }
        }
        
        Err(BenchError::ConnectionError(format!(
            "Failed to reconnect after {} attempts", MAX_RECONNECT_ATTEMPTS
        )))
    }
    
    /// Check transaction status with multiple fallback methods
    pub async fn check_transaction_status(&self, tx_hash: &str) -> Result<Option<u32>, BenchError> {
        // Try multiple methods to verify transaction status
        
        // Method 1: Try author_extrinsicStatus (substrate style)
        match self.check_tx_substrate_style(tx_hash).await {
            Ok(Some(block_number)) => {
                // Successfully found transaction
                return Ok(Some(block_number));
            }
            Ok(None) => {
                // Transaction not found with this method, try next method
            }
            Err(e) => {
                // Log error but try other methods
                debug!("Error checking transaction via substrate style: {}", e);
            }
        }
        
        // Method 2: Try ethereum style receipt check
        match self.check_tx_ethereum_style(tx_hash).await {
            Ok(Some(block_number)) => {
                return Ok(Some(block_number));
            }
            Ok(None) => {
                // Transaction not found with this method either, try next method
            }
            Err(e) => {
                debug!("Error checking transaction via ethereum style: {}", e);
            }
        }
        
        // Method 3: Check if in mempool
        match self.check_tx_in_mempool(tx_hash).await {
            Ok(true) => {
                // Transaction exists in mempool but not yet finalized
                return Ok(None);
            }
            Ok(false) => {
                // Transaction not found in mempool either
                debug!("Transaction {} not found in any source", tx_hash);
                return Ok(None);
            }
            Err(e) => {
                debug!("Error checking transaction in mempool: {}", e);
                // Fall through to return error
            }
        }
        
        // If all methods failed to find transaction, return None
        debug!("All transaction status check methods failed for {}", tx_hash);
        Ok(None)
    }
    
    // Method 1: Check transaction using Substrate style RPC
    async fn check_tx_substrate_style(&self, tx_hash: &str) -> Result<Option<u32>, BenchError> {
        let params = json!([tx_hash]);
        let request = json!({
            "jsonrpc": "2.0",
            "method": "author_extrinsicStatus",
            "params": params,
            "id": 1
        });
        
        let response = self.client.request::<serde_json::Value>(request).await?;
        
        if let Some(result) = response.get("result") {
            if let Some(status) = result.get("finalized") {
                if let Some(block_hash) = status.as_str() {
                    // Get block number from block hash
                    return self.get_block_number_by_hash(block_hash).await;
                }
            }
        }
        
        // Not finalized yet
        Ok(None)
    }
    
    // Method 2: Check transaction using Ethereum style RPC
    async fn check_tx_ethereum_style(&self, tx_hash: &str) -> Result<Option<u32>, BenchError> {
        let params = json!([tx_hash]);
        let request = json!({
            "jsonrpc": "2.0",
            "method": "eth_getTransactionReceipt",
            "params": params,
            "id": 1
        });
        
        let response = self.client.request::<serde_json::Value>(request).await?;
        
        if let Some(result) = response.get("result") {
            if !result.is_null() {
                // Transaction receipt exists
                if let Some(block_number_hex) = result.get("blockNumber").and_then(|v| v.as_str()) {
                    // Convert hex to number
                    return self.get_block_number_by_hash(block_number_hex).await;
                }
            }
        }
        
        // No receipt found
        Ok(None)
    }
    
    // Method 3: Check if transaction is in mempool
    async fn check_tx_in_mempool(&self, tx_hash: &str) -> Result<bool, BenchError> {
        let params = json!([tx_hash]);
        let request = json!({
            "jsonrpc": "2.0",
            "method": "eth_getTransactionByHash",
            "params": params,
            "id": 1
        });
        
        let response = self.client.request::<serde_json::Value>(request).await?;
        
        if let Some(result) = response.get("result") {
            if !result.is_null() {
                // Transaction exists in mempool or chain
                return Ok(true);
            }
        }
        
        // Transaction not found
        Ok(false)
    }
    
    // Helper to get block number from hash
    async fn get_block_number_by_hash(&self, block_hash: &str) -> Result<Option<u32>, BenchError> {
        let params = json!([block_hash]);
        let request = json!({
            "jsonrpc": "2.0",
            "method": "chain_getBlock",
            "params": params,
            "id": 1
        });
        
        let response = self.client.request::<serde_json::Value>(request).await?;
        
        if let Some(result) = response.get("result") {
            if let Some(block) = result.get("block") {
                if let Some(header) = block.get("header") {
                    if let Some(number_str) = header.get("number").and_then(|v| v.as_str()) {
                        // Remove '0x' prefix if present and parse
                        if let Ok(number) = parse_hex_to_u32(number_str) {
                            return Ok(Some(number));
                        }
                    }
                }
            }
        }
        
        Err(BenchError::ParseError(format!(
            "Could not extract block number from response: {:?}", response
        )))
    }

    pub async fn get_chain_metadata(&self) -> Result<ChainMetadata, BenchError> {
        let client = self.client.lock().await;
        
        // Get genesis hash
        let genesis_hash: String = match client.request("chain_getBlockHash", rpc_params![0]).await {
            Ok(hash) => hash,
            Err(e) => return Err(format!("Failed to get genesis hash: {}", e).into()),
        };
        
        // Get runtime version
        let runtime_version: serde_json::Value = match client.request("state_getRuntimeVersion", rpc_params![]).await {
            Ok(version) => version,
            Err(e) => return Err(format!("Failed to get runtime version: {}", e).into()),
        };
        
        let spec_version = runtime_version["specVersion"].as_u64().unwrap_or(0) as u32;
        let tx_version = runtime_version["transactionVersion"].as_u64().unwrap_or(0) as u32;
        
        // Get system properties
        let properties: serde_json::Value = match client.request("system_properties", rpc_params![]).await {
            Ok(props) => props,
            Err(e) => return Err(format!("Failed to get system properties: {}", e).into()),
        };
        
        let ss58_format = properties["ss58Format"].as_u64().unwrap_or(0) as u8;
        let token_decimals = properties["tokenDecimals"].as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_u64())
            .unwrap_or(18) as u8;
        
        let token_symbol = properties["tokenSymbol"].as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .unwrap_or("ASTR")
            .to_string();
        
        Ok(ChainMetadata {
            genesis_hash,
            runtime_version: spec_version,
            tx_version,
            ss58_format,
            token_decimals,
            token_symbol,
        })
    }
    
    // TRANSACTION SUBMISSION METHODS

    // Submit a transfer transaction
    pub async fn submit_transfer(&self, from: &str, to: &str, amount: u128) -> Result<String, BenchError> {
        let client = self.client.lock().await;
        
        // Create a hex-encoded transaction payload (simulating a signed extrinsic)
        let tx_payload = format!(
            "0x{}",
            hex::encode(format!(
                "transfer:{}:{}:{}",
                from,
                to,
                amount
            ).as_bytes())
        );
        
        let params = rpc_params![tx_payload];

        match client.request::<String, _>("author_submitExtrinsic", params).await {
            Ok(tx_hash) => Ok(tx_hash),
            Err(e) => Err(format!("Failed to submit transfer transaction: {}", e).into()),
        }
    }
    
    // Submit an ERC20 transfer transaction
    pub async fn submit_erc20_transfer(&self, from: &str, to: &str, amount: u128) -> Result<String, BenchError> {
        let client = self.client.lock().await;
        
        // Create a hex-encoded transaction payload (simulating a signed extrinsic)
        let tx_payload = format!(
            "0x{}",
            hex::encode(format!(
                "erc20_transfer:{}:{}:{}",
                from,
                to,
                amount
            ).as_bytes())
        );
        
        let params = rpc_params![tx_payload];

        match client.request::<String, _>("author_submitExtrinsic", params).await {
            Ok(tx_hash) => Ok(tx_hash),
            Err(e) => Err(format!("Failed to submit ERC20 transfer transaction: {}", e).into()),
        }
    }
    
    // Submit a complex contract call transaction
    pub async fn submit_complex_contract(&self, from: &str, amount: u128) -> Result<String, BenchError> {
        let client = self.client.lock().await;
        
        // Create a hex-encoded transaction payload (simulating a signed extrinsic)
        let tx_payload = format!(
            "0x{}",
            hex::encode(format!(
                "complex_contract:{}:{}",
                from,
                amount
            ).as_bytes())
        );
        
        let params = rpc_params![tx_payload];

        match client.request::<String, _>("author_submitExtrinsic", params).await {
            Ok(tx_hash) => Ok(tx_hash),
            Err(e) => Err(format!("Failed to submit complex contract transaction: {}", e).into()),
        }
    }
    
    // REAL TRANSACTION SUBMISSION METHODS
    
    // Submit a real transfer transaction
    pub async fn submit_real_transfer(&self, _account: &Account, to: &str, amount: u128) -> Result<String, BenchError> {
        // Make sure we have a seed phrase
        let seed_phrase = match &self.seed_phrase {
            Some(phrase) => phrase,
            None => return Err("Seed phrase is required for real transactions".into()),
        };
        
        println!("Submitting real transfer to {} with amount {}", to, amount);
        
        // Submit the real transaction with extended timeout (120 seconds)
        let transaction_result = tokio::time::timeout(
            Duration::from_secs(120), // 120 second timeout
            transaction::submit_real_transaction(
                &self.primary_url,
                seed_phrase,
                to,
                amount,
                &self.tx_script_dir,
            )
        ).await;
        
        match transaction_result {
            Ok(Ok(tx_hash)) => {
                println!("Transaction submitted successfully with hash: {}", tx_hash);
                Ok(tx_hash)
            },
            Ok(Err(e)) => {
                let error_str = format!("Transaction submission error: {}", e);
                println!("{}", error_str);
                Err(error_str.into())
            },
            Err(_) => {
                let error_str = "Transaction submission timed out after 120 seconds".to_string();
                println!("{}", error_str);
                Err(error_str.into())
            }
        }
    }
    
    // Submit a real ERC20 transfer transaction
    pub async fn submit_real_erc20_transfer(&self, _account: &Account, _to: &str, _amount: u128) -> Result<String, BenchError> {
        // Stub implementation - you would implement the real ERC20 transfer here
        Err("Real ERC20 transfers not yet implemented".into())
    }
    
    // Submit a real complex contract call transaction
    pub async fn submit_real_complex_contract(&self, _account: &Account, _amount: u128) -> Result<String, BenchError> {
        // Stub implementation - you would implement the real complex contract call here
        Err("Real complex contract calls not yet implemented".into())
    }
}

// Helper function to parse hex to u32
fn parse_hex_to_u32(hex_str: &str) -> Result<u32, BenchError> {
    let cleaned = hex_str.trim_start_matches("0x");
    u32::from_str_radix(cleaned, 16)
        .map_err(|e| BenchError::ParseError(format!("Failed to parse hex number: {}", e)))
}

// Generate a random recipient address for testing
pub fn generate_random_address() -> String {
    let mut bytes = [0u8; 32];
    thread_rng().fill(&mut bytes[..]);
    format!("0x{}", hex::encode(bytes))
} 