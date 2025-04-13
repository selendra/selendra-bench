use crate::{
    client::NodeClient,
    types::{Account, BenchmarkStats, TxType},
};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::{error::Error, sync::Arc, time::Duration};
use tokio::sync::{mpsc, Mutex};
use hex;

#[derive(Clone)]
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
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let tx_script_dir = "tx_scripts";
        let node_client = NodeClient::new(
            node_url,
            use_real_transactions,
            seed_phrase,
            tx_script_dir,
            min_amount,
            max_amount
        ).await?;
        
        let accounts = generate_accounts(num_accounts);
        
        Ok(Self {
            node_client,
            accounts,
            tx_type,
            target_tps,
            duration,
            use_real_transactions,
        })
    }

    pub async fn run(&self) -> Result<BenchmarkStats, Box<dyn Error + Send + Sync>> {
        println!("Getting chain metadata...");
        let metadata = self.node_client.get_chain_metadata().await?;
        let (tx_status_sender, mut tx_status_receiver) = mpsc::channel(1000);
        let mut stats = BenchmarkStats::default();
        stats.submitted = 0;

        println!("Starting transaction submission...");
        // Start transaction submission
        let submission_handle = tokio::spawn({
            let node_client = self.node_client.clone();
            let accounts = self.accounts.clone();
            let tx_type = self.tx_type.clone();
            let metadata = metadata.clone();
            let tx_status_sender = tx_status_sender.clone();
            let tps_target = self.target_tps;
            let duration = self.duration;
            
            async move {
                // Use a deterministic RNG for reproducibility
                let mut rng = StdRng::seed_from_u64(0);
                let mut submitted = 0;
                let total_txs = tps_target * duration as usize;
                
                println!("Will submit {} transactions over {} seconds", total_txs, duration);
                
                let start_time = std::time::Instant::now();
                while submitted < total_txs {
                    // Select a random account for the transaction
                    let account_idx = rng.gen_range(0..accounts.len());
                    let account = &accounts[account_idx];
                    
                    // Submit the transaction
                    if let Err(e) = node_client
                        .submit_transaction(account, &tx_type, &metadata, tx_status_sender.clone())
                        .await
                    {
                        eprintln!("Failed to submit transaction: {}", e);
                    } else {
                        submitted += 1;
                        if submitted % 10 == 0 || submitted == total_txs {
                            println!("Submitted {}/{} transactions ({:.1}%)", 
                                     submitted, total_txs, 
                                     (submitted as f64 / total_txs as f64) * 100.0);
                        }
                    }
                    
                    // Sleep to maintain TPS rate
                    let elapsed = start_time.elapsed();
                    let expected_elapsed = Duration::from_millis((1000 * submitted as u64) / tps_target as u64);
                    if elapsed < expected_elapsed {
                        tokio::time::sleep(expected_elapsed - elapsed).await;
                    }
                }
                println!("Transaction submission complete.");
                submitted
            }
        });

        println!("Processing transaction status updates...");
        // Process transaction status updates for a short time
        let status_timeout = tokio::time::timeout(
            std::time::Duration::from_secs(self.duration + 10), // Wait a bit longer than the benchmark duration
            async {
                while let Some(status) = tx_status_receiver.recv().await {
                    stats.submitted += 1;
                    if status.finalized {
                        stats.successful += 1;
                    } else if status.error.is_some() {
                        stats.failed += 1;
                        if let Some(error) = &status.error {
                            println!("Transaction error: {}", error);
                            *stats.errors.entry(error.clone()).or_insert(0) += 1;
                        }
                    }
                }
            }
        ).await;
        
        if status_timeout.is_err() {
            println!("Timeout waiting for transaction status updates");
        }

        println!("Waiting for submission to complete...");
        let submitted = submission_handle.await?;
        println!("Benchmark complete. Submitted {} transactions.", submitted);
        
        Ok(stats)
    }
}

fn generate_accounts(num: usize) -> Vec<Account> {
    let mut rng = StdRng::seed_from_u64(0);
    (0..num)
        .map(|_| {
            let mut private_key = vec![0u8; 32];
            rng.fill(&mut private_key[..]);
            let address = format!("0x{}", hex::encode(&private_key));
            Account {
                address,
                private_key,
                nonce: Arc::new(Mutex::new(0)),
            }
        })
        .collect()
} 