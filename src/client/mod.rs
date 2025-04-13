use crate::types::{Account, ChainMetadata, TransactionStatus, TxType};
use jsonrpsee::{
    core::client::ClientT,
    rpc_params,
    ws_client::{WsClient, WsClientBuilder},
};
use rand::{thread_rng, Rng};
use std::{error::Error, sync::Arc};
use tokio::sync::mpsc;
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

// Remove the generic From implementation since it conflicts with the blanket From<T> for T
// impl<E: Error> From<E> for BenchError {
//     fn from(e: E) -> Self {
//         BenchError(e.to_string())
//     }
// }

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
    client: Arc<WsClient>,
    tx_script_dir: String,
    use_real_transactions: bool,
    seed_phrase: Option<String>,
    min_amount: u128,
    max_amount: u128,
}

impl NodeClient {
    pub async fn new(url: &str, use_real_transactions: bool, seed_phrase: Option<String>, tx_script_dir: &str, min_amount: u128, max_amount: u128) -> Result<Self, BenchError> {
        let client = WsClientBuilder::default().build(url).await?;
        
        // If using real transactions, set up the transaction script
        if use_real_transactions {
            transaction::create_transaction_script(tx_script_dir)?;
        }
        
        Ok(Self {
            client: Arc::new(client),
            tx_script_dir: tx_script_dir.to_string(),
            use_real_transactions,
            seed_phrase,
            min_amount,
            max_amount,
        })
    }

    pub async fn submit_transaction(
        &self,
        account: &Account,
        tx_type: &TxType,
        metadata: &ChainMetadata,
        tx_status_sender: mpsc::Sender<TransactionStatus>,
    ) -> Result<(), BenchError> {
        if self.use_real_transactions {
            match tx_type {
                TxType::Transfer => self.submit_real_transfer(tx_status_sender).await,
                // For now, we'll only implement real transfers
                _ => self.submit_simulated_transaction(account, tx_type, metadata, tx_status_sender).await,
            }
        } else {
            self.submit_simulated_transaction(account, tx_type, metadata, tx_status_sender).await
        }
    }
    
    async fn submit_real_transfer(
        &self,
        tx_status_sender: mpsc::Sender<TransactionStatus>,
    ) -> Result<(), BenchError> {
        // Make sure we have a seed phrase
        let seed_phrase = match &self.seed_phrase {
            Some(phrase) => phrase,
            None => return Err("Seed phrase is required for real transactions".into()),
        };
        
        // Generate a random amount within the specified range
        let amount = {
            let mut rng = thread_rng();
            rng.gen_range(self.min_amount..=self.max_amount)
        };  // This scope ensures rng is dropped before the await
        
        // Generate a random recipient (in a real scenario, you might want to use a fixed address that you control)
        let recipient = transaction::generate_random_recipient();
        
        // Get the node URL from the client
        let node_url = "wss://rpcx.selendra.org".to_string(); // TODO: Extract this from self.client
        
        match transaction::submit_real_transaction(
            &node_url,
            seed_phrase,
            &recipient,
            amount,
            &self.tx_script_dir,
        ).await {
            Ok(tx_hash) => {
                let status = TransactionStatus {
                    tx_hash,
                    block_number: None,
                    finalized: true,  // For simplicity, we'll assume it's finalized
                    error: None,
                };
                
                let _ = tx_status_sender.send(status).await;
                Ok(())
            },
            Err(e) => {
                let error_str = e.to_string();
                let status = TransactionStatus {
                    tx_hash: "failed".to_string(),
                    block_number: None,
                    finalized: false,
                    error: Some(error_str),
                };
                
                let _ = tx_status_sender.send(status).await;
                Ok(())
            }
        }
    }

    async fn submit_simulated_transaction(
        &self,
        account: &Account,
        tx_type: &TxType,
        metadata: &ChainMetadata,
        tx_status_sender: mpsc::Sender<TransactionStatus>,
    ) -> Result<(), BenchError> {
        match tx_type {
            TxType::Transfer => self.submit_transfer(account, metadata, tx_status_sender).await,
            TxType::Erc20Transfer => self.submit_erc20_transfer(account, metadata, tx_status_sender).await,
            TxType::ComplexContract => self.submit_complex_contract(account, metadata, tx_status_sender).await,
        }
    }

    async fn submit_transfer(
        &self,
        account: &Account,
        _metadata: &ChainMetadata,
        tx_status_sender: mpsc::Sender<TransactionStatus>,
    ) -> Result<(), BenchError> {
        let mut nonce = account.nonce.lock().await;
        
        // Create a hex-encoded transaction payload (simulating a signed extrinsic)
        let tx_payload = format!(
            "0x{}",
            hex::encode(format!(
                "transfer:{}:{}:{}",
                account.address,
                generate_random_address(),
                *nonce
            ).as_bytes())
        );
        
        let params = rpc_params![tx_payload];

        match self.client.request::<String, _>("author_submitExtrinsic", params).await {
            Ok(tx_hash) => {
                *nonce += 1;
                
                let status = TransactionStatus {
                    tx_hash,
                    block_number: None,
                    finalized: false,
                    error: None,
                };

                let _ = tx_status_sender.send(status).await;
                Ok(())
            },
            Err(e) => {
                let error_str = e.to_string();
                let status = TransactionStatus {
                    tx_hash: "failed".to_string(),
                    block_number: None,
                    finalized: false,
                    error: Some(error_str),
                };
                
                let _ = tx_status_sender.send(status).await;
                Ok(())
            }
        }
    }

    async fn submit_erc20_transfer(
        &self,
        account: &Account,
        _metadata: &ChainMetadata,
        tx_status_sender: mpsc::Sender<TransactionStatus>,
    ) -> Result<(), BenchError> {
        let mut nonce = account.nonce.lock().await;
        
        // Create a hex-encoded transaction payload (simulating a signed extrinsic)
        let tx_payload = format!(
            "0x{}",
            hex::encode(format!(
                "erc20_transfer:{}:{}:{}:{}",
                account.address,
                generate_random_address(),
                generate_random_address(), // contract address
                *nonce
            ).as_bytes())
        );
        
        let params = rpc_params![tx_payload];

        match self.client.request::<String, _>("author_submitExtrinsic", params).await {
            Ok(tx_hash) => {
                *nonce += 1;
                
                let status = TransactionStatus {
                    tx_hash,
                    block_number: None,
                    finalized: false,
                    error: None,
                };

                let _ = tx_status_sender.send(status).await;
                Ok(())
            },
            Err(e) => {
                let error_str = e.to_string();
                let status = TransactionStatus {
                    tx_hash: "failed".to_string(),
                    block_number: None,
                    finalized: false,
                    error: Some(error_str),
                };
                
                let _ = tx_status_sender.send(status).await;
                Ok(())
            }
        }
    }

    async fn submit_complex_contract(
        &self,
        account: &Account,
        _metadata: &ChainMetadata,
        tx_status_sender: mpsc::Sender<TransactionStatus>,
    ) -> Result<(), BenchError> {
        let mut nonce = account.nonce.lock().await;
        
        // Create a hex-encoded transaction payload (simulating a signed extrinsic)
        let tx_payload = format!(
            "0x{}",
            hex::encode(format!(
                "complex_contract:{}:{}:execute:arg1,arg2:{}",
                account.address,
                generate_random_address(), // contract address
                *nonce
            ).as_bytes())
        );
        
        let params = rpc_params![tx_payload];

        match self.client.request::<String, _>("author_submitExtrinsic", params).await {
            Ok(tx_hash) => {
                *nonce += 1;
                
                let status = TransactionStatus {
                    tx_hash,
                    block_number: None,
                    finalized: false,
                    error: None,
                };

                let _ = tx_status_sender.send(status).await;
                Ok(())
            },
            Err(e) => {
                let error_str = e.to_string();
                let status = TransactionStatus {
                    tx_hash: "failed".to_string(),
                    block_number: None,
                    finalized: false,
                    error: Some(error_str),
                };
                
                let _ = tx_status_sender.send(status).await;
                Ok(())
            }
        }
    }

    pub async fn get_chain_metadata(&self) -> Result<ChainMetadata, BenchError> {
        let genesis_hash: String = self.client.request("chain_getBlockHash", rpc_params![0]).await?;
        
        // These methods might return JSON objects, so we'll handle them with serde_json::Value
        let runtime_version_value: serde_json::Value = self.client.request("state_getRuntimeVersion", rpc_params![]).await?;
        let runtime_version = runtime_version_value["specVersion"].as_u64().unwrap_or(0) as u32;
        let tx_version = runtime_version_value["transactionVersion"].as_u64().unwrap_or(0) as u32;
        
        Ok(ChainMetadata {
            genesis_hash,
            runtime_version,
            tx_version,
            ss58_format: 42, // Default SS58 format
            token_decimals: 12,
            token_symbol: "SEL".to_string(),
        })
    }
}

fn generate_random_address() -> String {
    let mut rng = thread_rng();
    let mut bytes = vec![0u8; 32];
    rng.fill(&mut bytes[..]);
    format!("0x{}", hex::encode(bytes))
} 