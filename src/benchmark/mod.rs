use crate::{
    client::NodeClient,
    types::{Account, BenchmarkStats, TransactionStatus, TxType},
};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::{
    collections::HashMap,
    error::Error, 
    sync::Arc, 
    time::{Duration, Instant}
};
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
    min_amount: u128,
    max_amount: u128,
    stats: BenchmarkStats,
    connection_retry_interval: u64, // New field for periodic reconnection
    reconnect_after_errors: u32,    // Number of consecutive errors before reconnection
    error_count: u32,               // Track consecutive errors
}

impl BenchmarkRunner {
    pub async fn new(
        node_url: &str,
        backup_url: Option<String>,
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
        
        // If there's a backup URL, construct a URL string with both for the NodeClient
        let effective_url = if let Some(backup) = &backup_url {
            format!("{},{}", node_url, backup)
        } else {
            node_url.to_string()
        };
        
        let node_client = NodeClient::new(
            &effective_url,
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
            min_amount,
            max_amount,
            stats: BenchmarkStats::default(),
            connection_retry_interval: 1800, // Reconnect every 30 minutes by default
            reconnect_after_errors: 5,       // Reconnect after 5 consecutive errors
            error_count: 0,                  // Start with 0 errors
        })
    }

    pub async fn run(&mut self) -> Result<BenchmarkStats, Box<dyn Error + Send + Sync>> {
        println!("Getting chain metadata...");
        let _ = self.node_client.get_chain_metadata().await;

        println!("Starting transaction submission...");

        // Create a channel for transaction status updates
        let (tx_sender, tx_receiver) = mpsc::channel::<TransactionStatus>(100);
        
        // Add a buffer time after all transactions are submitted
        let wait_after_completion = 120; // seconds
        let total_wait_time = self.duration + wait_after_completion;
        
        println!("Waiting for transaction status updates for up to {} seconds...", total_wait_time);
        
        // Calculate the number of transactions to submit
        let total_transactions = self.target_tps as u64 * self.duration;
        println!("Will submit {} transactions over {} seconds", total_transactions, self.duration);
        
        // Determine max concurrent submissions based on whether real transactions are used
        let max_concurrent_submissions = if self.use_real_transactions {
            // For real transactions, limit concurrency to prevent overloading the node
            if self.target_tps <= 1 {
                1 // For very low TPS, just do one at a time
            } else if self.target_tps <= 5 {
                2 // For low TPS, allow two concurrent submissions
            } else {
                3 // For higher TPS, allow more concurrent submissions
            }
        } else {
            // For simulated transactions, allow more concurrency
            10
        };
        
        // Keep track of in-flight transactions and status
        let mut in_flight_count = 0;
        let mut transactions_submitted = 0;
        let transactions_completed = 0;
        let mut transactions_failed = 0;
        let mut last_reconnect_time = Instant::now();
        let mut last_status_print_time = Instant::now();
        
        // Start the transaction processing loop
        let start_time = Instant::now();
        let mut next_tx_time = start_time;
        let tx_interval = if total_transactions > 0 {
            Duration::from_secs_f64(self.duration as f64 / total_transactions as f64)
        } else {
            Duration::from_secs(1)
        };
        
        // Process transaction status updates from the channel
        let mut final_stats = BenchmarkStats::default();
        final_stats.transaction_statuses = Vec::new();
        
        // Create a separate task to collect transaction statuses
        let status_collector_handle = {
            let mut tx_receiver = tx_receiver;
            let total_wait_time = total_wait_time;
            
            tokio::spawn(async move {
                let mut collected_stats = BenchmarkStats::default();
                collected_stats.transaction_statuses = Vec::new();
                let start_time = Instant::now();
                
                while let Some(tx_status) = tx_receiver.recv().await {
                    // Add transaction status to statistics
                    collected_stats.transaction_statuses.push(tx_status);
                    
                    // Check if we've been waiting long enough
                    if start_time.elapsed().as_secs() > total_wait_time {
                        println!("Reached maximum wait time, stopping status processing");
                        break;
                    }
                }
                
                collected_stats
            })
        };
        
        // Main transaction submission loop
        while transactions_submitted < total_transactions {
            // Wait until it's time for the next transaction
            let now = Instant::now();
            if now < next_tx_time {
                tokio::time::sleep(next_tx_time - now).await;
            }
            
            // Check if we have too many in-flight transactions
            while in_flight_count >= max_concurrent_submissions {
                // Sleep a bit and check again to avoid busy-wait
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            
            in_flight_count += 1;
            transactions_submitted += 1;
            
            // Periodically try to reconnect to maintain connection
            if last_reconnect_time.elapsed().as_secs() >= self.connection_retry_interval {
                println!("Performing periodic reconnection to node...");
                match self.node_client.reconnect().await {
                    Ok(_) => {
                        println!("Successfully reconnected to node");
                        self.error_count = 0;
                        last_reconnect_time = Instant::now();
                    },
                    Err(e) => {
                        println!("Failed to reconnect to node: {}", e);
                        // Don't increment error count here, just log
                    }
                }
            }
            
            // Check if we need to reconnect due to errors
            if self.error_count >= self.reconnect_after_errors {
                println!("Too many consecutive errors ({}), attempting to reconnect...", self.error_count);
                match self.node_client.reconnect().await {
                    Ok(_) => {
                        println!("Successfully reconnected to node after errors");
                        self.error_count = 0;
                        last_reconnect_time = Instant::now();
                    },
                    Err(e) => {
                        println!("Failed to reconnect to node after errors: {}", e);
                        // Continue anyway, we'll try again after more errors
                    }
                }
            }

            // Select a random account and amount for this transaction
            let account_idx = rand::thread_rng().gen_range(0..self.accounts.len());
            let account = &self.accounts[account_idx];
            let amount = if self.min_amount == self.max_amount {
                self.min_amount
            } else {
                rand::thread_rng().gen_range(self.min_amount..self.max_amount)
            };
            
            // Submit the transaction based on the specified type
            let recipient = crate::client::transaction::generate_random_recipient();
            println!("Submitting {} transaction to {} with amount {}", 
                if self.use_real_transactions { "real" } else { "simulated" },
                recipient,
                amount);
            
            let tx_result = match self.tx_type {
                TxType::Transfer => {
                    if self.use_real_transactions {
                        self.node_client.submit_real_transfer(
                            account, 
                            &recipient, 
                            amount
                        ).await
                    } else {
                        self.node_client.submit_transfer(
                            &account.address, 
                            &recipient, 
                            amount
                        ).await
                    }
                },
                TxType::Erc20Transfer => {
                    if self.use_real_transactions {
                        self.node_client.submit_real_erc20_transfer(
                            account,
                            &recipient,
                            amount
                        ).await
                    } else {
                        self.node_client.submit_erc20_transfer(
                            &account.address,
                            &recipient,
                            amount
                        ).await
                    }
                },
                TxType::ComplexContract => {
                    if self.use_real_transactions {
                        self.node_client.submit_real_complex_contract(
                            account,
                            amount
                        ).await
                    } else {
                        self.node_client.submit_complex_contract(
                            &account.address,
                            amount
                        ).await
                    }
                }
            };
            
            // Process the result of the transaction submission
            match tx_result {
                Ok(tx_hash) => {
                    println!("Transaction submitted successfully with hash: {}", tx_hash);
                    // Send the initial transaction status through the channel
                    let _ = tx_sender.send(TransactionStatus {
                        tx_hash: tx_hash.clone(),
                        block_number: None,
                        status: "pending".to_string(),
                    }).await;
                    
                    // Spawn a new task to check the transaction status periodically
                    // Since we can't pass self into an async closure, we need to create a clone of the client
                    let node_client_clone = self.node_client.clone();
                    let tx_sender_clone = tx_sender.clone();
                    let tx_hash_clone = tx_hash.clone();
                    
                    tokio::spawn(async move {
                        let mut attempts = 0;
                        let max_attempts = 60; // 60 attempts * 5 seconds = 300 seconds (5 minutes)
                        
                        loop {
                            if attempts >= max_attempts {
                                // Transaction status check timed out
                                let _ = tx_sender_clone.send(TransactionStatus {
                                    tx_hash: tx_hash_clone.clone(),
                                    block_number: None,
                                    status: "timeout".to_string(),
                                }).await;
                                break;
                            }
                            
                            // Check the transaction status
                            match node_client_clone.check_transaction_status(&tx_hash_clone).await {
                                Ok(Some(block_number)) => {
                                    // Transaction is confirmed
                                    let _ = tx_sender_clone.send(TransactionStatus {
                                        tx_hash: tx_hash_clone.clone(),
                                        block_number: Some(block_number),
                                        status: "confirmed".to_string(),
                                    }).await;
                                    break;
                                },
                                Ok(None) => {
                                    // Transaction still pending, wait and check again
                                    tokio::time::sleep(Duration::from_secs(5)).await;
                                    attempts += 1;
                                },
                                Err(e) => {
                                    println!("Error checking transaction status: {}", e);
                                    // Wait and retry
                                    tokio::time::sleep(Duration::from_secs(5)).await;
                                    attempts += 1;
                                }
                            }
                        }
                    });
                },
                Err(e) => {
                    println!("Failed to submit transaction: {}", e);
                    self.error_count += 1;
                    in_flight_count -= 1;
                    transactions_failed += 1;
                    
                    // Record the failed submission
                    let _ = tx_sender.send(TransactionStatus {
                        tx_hash: "failed_to_submit".to_string(),
                        block_number: None,
                        status: format!("error: {}", e),
                    }).await;
                    
                    // If there are too many consecutive errors, we might want to slow down
                    if self.error_count >= self.reconnect_after_errors / 2 {
                        println!("Experiencing errors, slowing down submission rate...");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
            
            // Print status update periodically
            if last_status_print_time.elapsed().as_secs() >= 60 {  // Every minute
                println!("Status: Submitted={}, Completed={}, Failed={}, In-flight={}",
                    transactions_submitted, transactions_completed, transactions_failed, in_flight_count);
                last_status_print_time = Instant::now();
            }
            
            // Calculate time for next transaction
            next_tx_time += tx_interval;
        }
        
        println!("Finished submitting all {} transactions", transactions_submitted);

        // Wait some time for ongoing transactions to complete
        println!("Waiting for transaction statuses to be collected...");
        tokio::time::sleep(Duration::from_secs(wait_after_completion)).await;

        // Close the sender channel to signal the status collector to finish
        drop(tx_sender);

        // Wait for the status collector to finish and get the stats
        match status_collector_handle.await {
            Ok(stats) => {
                println!("Benchmark completed");
                println!("Total transactions submitted: {}", stats.transaction_statuses.len());
                
                // Count confirmed transactions
                let confirmed_count = stats.transaction_statuses.iter()
                    .filter(|s| s.status == "confirmed")
                    .count();
                println!("Confirmed transactions: {}", confirmed_count);
                
                // Count and display errors by type
                let mut error_counts = std::collections::HashMap::new();
                for status in &stats.transaction_statuses {
                    if status.status.starts_with("error:") {
                        *error_counts.entry(&status.status).or_insert(0) += 1;
                    }
                }
                
                println!("Error breakdown:");
                for (error, count) in error_counts {
                    println!("  {}: {}", error, count);
                }
                
                Ok(stats)
            },
            Err(e) => {
                println!("Error collecting transaction statuses: {}", e);
                Ok(final_stats)
            }
        }
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