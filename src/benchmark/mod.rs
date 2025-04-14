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
use std::sync::atomic::{AtomicBool, Ordering};

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

    pub async fn run(&mut self) -> BenchmarkStats {
        println!("Starting benchmark with {} accounts, TPS target: {}, Duration: {}s",
                 self.accounts.len(), self.target_tps, self.duration);
                 
        let start_time = std::time::Instant::now();
        let end_time = start_time + std::time::Duration::from_secs(self.duration);
        
        // Create a channel for transaction status updates - use larger buffer
        let (tx_sender, mut tx_receiver) = 
            tokio::sync::mpsc::channel::<(String, TransactionStatus)>(2000);
            
        // Track submitted transactions and their timestamps
        let mut submitted_txs: HashMap<String, std::time::Instant> = HashMap::new();
        let mut confirmed_txs: HashMap<String, u32> = HashMap::new();
        let mut errored_txs: HashMap<String, String> = HashMap::new();
        
        // Adaptive parameters
        let mut max_wait_time = std::time::Duration::from_secs(180); // 3 minutes
        let timeout_per_tx = std::time::Duration::from_secs(60); // 1 minute
        let mut max_concurrent = 5;
        
        // Current state
        let mut in_flight_count = 0;
        let mut last_status_time = std::time::Instant::now();
        let mut total_submitted = 0;
        let mut reconnect_attempts = 0;
        let max_reconnect_attempts = 3;
        
        // Setup a separate task for status checking
        let node_client_clone = self.node_client.clone();
        let check_status_handle = tokio::spawn(async move {
            let mut check_interval = tokio::time::interval(std::time::Duration::from_secs(2));
            let mut tx_hashes: Vec<String> = Vec::new();
            let mut tx_status: HashMap<String, Option<u32>> = HashMap::new();
            
            loop {
                check_interval.tick().await;
                
                // Calculate dynamic check interval based on number of pending txs
                let new_interval = match tx_hashes.len() {
                    0..=10 => std::time::Duration::from_secs(2),
                    11..=100 => std::time::Duration::from_secs(5),
                    101..=500 => std::time::Duration::from_secs(10),
                    _ => std::time::Duration::from_secs(15),
                };
                check_interval = tokio::time::interval(new_interval);
                
                // Process each tx to check status
                let mut i = 0;
                while i < tx_hashes.len() {
                    let hash = &tx_hashes[i];
                    
                    // Check transaction status
                    match node_client_clone.check_transaction_status(hash).await {
                        Ok(Some(block_number)) => {
                            // Transaction confirmed
                            println!("Transaction {} confirmed in block {}", hash, block_number);
                            if let Err(e) = tx_sender.send((hash.clone(), TransactionStatus::Confirmed(block_number))).await {
                                println!("Error sending confirmation: {}", e);
                            }
                            
                            // Remove from list to check
                            tx_hashes.swap_remove(i);
                            tx_status.remove(hash);
                            continue; // Don't increment i since we swapped
                        },
                        Ok(None) => {
                            // Still pending
                            i += 1;
                        },
                        Err(e) => {
                            println!("Error checking tx {}: {}", hash, e);
                            
                            // After a certain number of errors, consider tx as failed
                            let error_count = tx_status.entry(hash.clone()).or_insert(None);
                            match error_count {
                                Some(count) => {
                                    let new_count = count + 1;
                                    if new_count > 5 {
                                        // Too many errors, mark as failed
                                        if let Err(e) = tx_sender.send((hash.clone(), TransactionStatus::Error(format!("Failed after {} attempts: {}", new_count, e)))).await {
                                            println!("Error sending tx error status: {}", e);
                                        }
                                        tx_hashes.swap_remove(i);
                                        tx_status.remove(hash);
                                        continue;
                                    } else {
                                        *error_count = Some(new_count);
                                    }
                                },
                                None => {
                                    *error_count = Some(1);
                                }
                            }
                            i += 1;
                        }
                    }
                }
                
                // Check if channel is closed
                if tx_sender.is_closed() {
                    break;
                }
            }
        });
        
        // Main transaction submission loop
        while std::time::Instant::now() < end_time {
            // Check if we have capacity for more transactions
            if in_flight_count >= max_concurrent {
                // Wait for status updates
                match tokio::time::timeout(std::time::Duration::from_millis(100), tx_receiver.recv()).await {
                    Ok(Some((tx_hash, status))) => {
                        self.handle_tx_status(&tx_hash, status, &mut submitted_txs, 
                                             &mut confirmed_txs, &mut errored_txs, &mut in_flight_count);
                    },
                    Ok(None) => {
                        // Channel closed - should not happen
                        println!("Status channel closed unexpectedly");
                        break;
                    },
                    Err(_) => {
                        // Timeout - continue to check if we should submit more
                    }
                }
                continue;
            }
            
            // Calculate sleep time to maintain TPS
            let sleep_time = if self.target_tps > 0 {
                let desired_interval = std::time::Duration::from_secs_f64(1.0 / self.target_tps as f64);
                desired_interval
            } else {
                std::time::Duration::from_millis(0)
            };
            
            // Submit a transaction
            let account_idx = total_submitted % self.accounts.len();
            let account = &self.accounts[account_idx];
            
            match self.submit_transaction(&account).await {
                Ok(tx_hash) => {
                    println!("Submitted transaction {}: {}", total_submitted + 1, tx_hash);
                    submitted_txs.insert(tx_hash.clone(), std::time::Instant::now());
                    in_flight_count += 1;
                    total_submitted += 1;
                    
                    // Add to status checking task
                    if let Err(e) = tx_sender.send((tx_hash, TransactionStatus::Submitted)).await {
                        println!("Error adding tx to status checker: {}", e);
                    }
                },
                Err(e) => {
                    println!("Error submitting transaction: {}", e);
                    
                    // Try to reconnect if submission fails
                    if reconnect_attempts < max_reconnect_attempts {
                        println!("Attempting to reconnect to node ({}/{})", 
                                reconnect_attempts + 1, max_reconnect_attempts);
                        
                        if let Err(e) = self.node_client.reconnect().await {
                            println!("Failed to reconnect: {}", e);
                        } else {
                            println!("Successfully reconnected");
                        }
                        reconnect_attempts += 1;
                        
                        // Backoff a bit before retrying
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    } else {
                        println!("Max reconnection attempts reached, stopping benchmark");
                        break;
                    }
                }
            }
            
            // Print periodic status updates every 15 seconds
            if last_status_time.elapsed() > std::time::Duration::from_secs(15) {
                println!("Status: Submitted={}, Confirmed={}, Errors={}, In-flight={}",
                    total_submitted, confirmed_txs.len(), errored_txs.len(), in_flight_count);
                last_status_time = std::time::Instant::now();
            }
            
            // Sleep to maintain TPS
            tokio::time::sleep(sleep_time).await;
            
            // Process any status updates
            while let Ok(Some((tx_hash, status))) = tokio::time::timeout(
                std::time::Duration::from_millis(0), 
                tx_receiver.recv()
            ).await {
                self.handle_tx_status(&tx_hash, status, &mut submitted_txs, 
                                     &mut confirmed_txs, &mut errored_txs, &mut in_flight_count);
            }
        }
        
        println!("Submission period completed, waiting for in-flight transactions to complete...");
        
        // After submission period, wait for in-flight transactions to complete
        let wait_start = std::time::Instant::now();
        
        // Check for timed-out transactions
        let mut timed_out = false;
        while in_flight_count > 0 && !timed_out {
            // Calculate remaining wait time
            let elapsed = wait_start.elapsed();
            if elapsed >= max_wait_time {
                println!("Maximum wait time reached, finalizing benchmark");
                timed_out = true;
                break;
            }
            
            // Check for timed-out individual transactions
            let current_time = std::time::Instant::now();
            let mut timeout_txs = Vec::new();
            
            for (hash, submit_time) in &submitted_txs {
                if current_time.duration_since(*submit_time) > timeout_per_tx {
                    timeout_txs.push(hash.clone());
                }
            }
            
            for hash in timeout_txs {
                println!("Transaction {} timed out", hash);
                errored_txs.insert(hash.clone(), "Transaction timed out".to_string());
                submitted_txs.remove(&hash);
                in_flight_count -= 1;
            }
            
            // Process status updates
            match tokio::time::timeout(
                std::time::Duration::from_secs(5), 
                tx_receiver.recv()
            ).await {
                Ok(Some((tx_hash, status))) => {
                    self.handle_tx_status(&tx_hash, status, &mut submitted_txs, 
                                         &mut confirmed_txs, &mut errored_txs, &mut in_flight_count);
                },
                Ok(None) => {
                    // Channel closed
                    println!("Status channel closed");
                    break;
                },
                Err(_) => {
                    // Timeout - print progress
                    println!("Waiting for {} transactions, elapsed: {:.2}s/{:.2}s", 
                             in_flight_count, 
                             elapsed.as_secs_f64(),
                             max_wait_time.as_secs_f64());
                }
            }
        }
        
        // Close the status channel and join the status checking task
        drop(tx_sender);
        if let Err(e) = check_status_handle.await {
            println!("Error joining status checker task: {}", e);
        }
        
        // Calculate statistics
        let total_time_ms = start_time.elapsed().as_millis() as u64;
        let actual_tps = if total_time_ms > 0 {
            (total_submitted as f64 * 1000.0) / (total_time_ms as f64)
        } else {
            0.0
        };
        
        // Calculate confirmation times
        let mut confirmation_times = Vec::new();
        let mut total_time = 0u64;
        
        for (hash, block_number) in &confirmed_txs {
            if let Some(submit_time) = submitted_txs.get(hash) {
                let conf_time = submit_time.elapsed().as_millis() as u64;
                confirmation_times.push(conf_time);
                total_time += conf_time;
            }
        }
        
        let avg_confirmation_time = if !confirmation_times.is_empty() {
            total_time / confirmation_times.len() as u64
        } else {
            0
        };
        
        // Sort confirmation times for percentile calculation
        confirmation_times.sort();
        
        let p50 = if !confirmation_times.is_empty() {
            confirmation_times[confirmation_times.len() / 2]
        } else {
            0
        };
        
        let p90 = if !confirmation_times.is_empty() {
            confirmation_times[(confirmation_times.len() * 9) / 10]
        } else {
            0
        };
        
        let p99 = if !confirmation_times.is_empty() {
            confirmation_times[(confirmation_times.len() * 99) / 100]
        } else {
            0
        };
        
        // Create benchmark stats
        let stats = BenchmarkStats {
            total_transactions: total_submitted,
            successful_transactions: confirmed_txs.len() as u32,
            failed_transactions: errored_txs.len() as u32,
            tps: actual_tps,
            target_tps: self.target_tps as f64,
            duration: self.duration,
            avg_confirmation_time,
            p50_confirmation_time: p50,
            p90_confirmation_time: p90,
            p99_confirmation_time: p99,
            start_time: start_time.elapsed().as_secs(),
            errors: errored_txs,
            tx_type: self.tx_type,
        };
        
        // Print summary
        println!("\nBenchmark Summary:");
        println!("------------------");
        println!("Total Transactions: {}", stats.total_transactions);
        println!("Successful Transactions: {} ({:.2}%)", 
                 stats.successful_transactions,
                 (stats.successful_transactions as f64 / stats.total_transactions as f64) * 100.0);
        println!("Failed Transactions: {} ({:.2}%)", 
                 stats.failed_transactions,
                 (stats.failed_transactions as f64 / stats.total_transactions as f64) * 100.0);
        println!("Actual TPS: {:.2}", stats.tps);
        println!("Target TPS: {:.2}", stats.target_tps);
        println!("Duration: {}s", stats.duration);
        println!("Average Confirmation Time: {}ms", stats.avg_confirmation_time);
        println!("P50 Confirmation Time: {}ms", stats.p50_confirmation_time);
        println!("P90 Confirmation Time: {}ms", stats.p90_confirmation_time);
        println!("P99 Confirmation Time: {}ms", stats.p99_confirmation_time);
        
        stats
    }
    
    // Helper function to handle transaction status updates
    fn handle_tx_status(
        &self,
        tx_hash: &str,
        status: TransactionStatus,
        submitted_txs: &mut HashMap<String, std::time::Instant>,
        confirmed_txs: &mut HashMap<String, u32>,
        errored_txs: &mut HashMap<String, String>,
        in_flight_count: &mut usize,
    ) {
        match status {
            TransactionStatus::Confirmed(block_number) => {
                if submitted_txs.contains_key(tx_hash) {
                    let submit_time = submitted_txs.remove(tx_hash).unwrap();
                    let confirmation_time = submit_time.elapsed().as_millis();
                    println!("Transaction {} confirmed in block {} (took {}ms)",
                             tx_hash, block_number, confirmation_time);
                    confirmed_txs.insert(tx_hash.to_string(), block_number);
                    *in_flight_count -= 1;
                }
            },
            TransactionStatus::Error(error) => {
                if submitted_txs.contains_key(tx_hash) {
                    println!("Transaction {} failed: {}", tx_hash, error);
                    submitted_txs.remove(tx_hash);
                    errored_txs.insert(tx_hash.to_string(), error);
                    *in_flight_count -= 1;
                }
            },
            TransactionStatus::Submitted => {
                // Just tracking submission, no status change
            },
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