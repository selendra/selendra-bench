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

// Add the transaction module
pub mod transaction;

// Custom error type that can be sent between threads
#[derive(Debug)]
pub struct BenchError(String);

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

#[derive(Clone)]
pub struct NodeClient {
    active_client: Arc<Mutex<WsClient>>,
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
            active_client: Arc::new(Mutex::new(client)),
            primary_url,
            backup_urls,
            tx_script_dir: absolute_script_dir,
            use_real_transactions,
            seed_phrase,
            min_amount,
            max_amount,
        })
    }
    
    // Method to reconnect to any available RPC endpoint
    pub async fn reconnect(&self) -> Result<(), BenchError> {
        let mut urls = vec![self.primary_url.clone()];
        urls.extend(self.backup_urls.clone());
        
        let mut last_error = None;
        
        for url in urls {
            match WsClientBuilder::default().build(&url).await {
                Ok(client) => {
                    println!("Successfully reconnected to URL: {}", url);
                    let mut active_client = self.active_client.lock().await;
                    *active_client = client;
                    return Ok(());
                },
                Err(e) => {
                    println!("Failed to reconnect to URL {}: {}", url, e);
                    last_error = Some(e);
                }
            }
        }
        
        Err(format!("Failed to reconnect to any RPC endpoint: {}", last_error.unwrap()).into())
    }
    
    // Method to check if a transaction has been included in a block
    pub async fn check_transaction_status(&self, tx_hash: &str) -> Result<Option<u32>, BenchError> {
        let params = rpc_params![tx_hash];
        let client = self.active_client.lock().await;
        
        match client.request::<serde_json::Value, _>("author_extrinsicStatus", params).await {
            Ok(status) => {
                // Parse the response to check if the transaction is in a block
                if let Some(block_info) = status.get("inBlock") {
                    if let Some(block_hash) = block_info.as_str() {
                        // Get the block number from the block hash
                        let params = rpc_params![block_hash];
                        match client.request::<serde_json::Value, _>("chain_getBlock", params).await {
                            Ok(block) => {
                                if let Some(header) = block.get("block").and_then(|b| b.get("header")) {
                                    if let Some(number) = header.get("number").and_then(|n| n.as_str()) {
                                        // Convert hex block number to u32
                                        if let Ok(number) = u32::from_str_radix(number.trim_start_matches("0x"), 16) {
                                            return Ok(Some(number));
                                        }
                                    }
                                }
                                // If we couldn't parse the block number, return Some(0) to indicate it's in a block
                                return Ok(Some(0));
                            },
                            Err(e) => {
                                println!("Error getting block details: {}", e);
                                // Return Some(0) to indicate it's in a block even if we couldn't get details
                                return Ok(Some(0));
                            }
                        }
                    }
                }
                // Transaction is not in a block yet
                Ok(None)
            },
            Err(e) => {
                // If the error suggests the transaction is not found, it might still be pending
                if e.to_string().contains("not found") || e.to_string().contains("Unknown") {
                    return Ok(None);
                }
                Err(format!("Error checking transaction status: {}", e).into())
            }
        }
    }

    pub async fn get_chain_metadata(&self) -> Result<ChainMetadata, BenchError> {
        let client = self.active_client.lock().await;
        
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
        let client = self.active_client.lock().await;
        
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
        let client = self.active_client.lock().await;
        
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
        let client = self.active_client.lock().await;
        
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

// Generate a random recipient address for testing
pub fn generate_random_address() -> String {
    let mut bytes = [0u8; 32];
    thread_rng().fill(&mut bytes[..]);
    format!("0x{}", hex::encode(bytes))
} 