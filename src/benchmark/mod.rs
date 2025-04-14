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
use tokio::sync::{mpsc, Mutex, watch};
use hex;
use std::sync::atomic::{AtomicBool, Ordering};
use log::{debug, error, info, warn};
use crate::client::{BenchError};

const MAX_STATUS_CHECK_RETRIES: u8 = 5;
const STATUS_CHECK_INITIAL_DELAY_MS: u64 = 500;
const MAX_WAIT_TIME_MINUTES: u64 = 10;
const PROGRESS_LOG_INTERVAL_SECONDS: u64 = 15;

#[derive(Clone)]
pub struct BenchmarkRunner {
    node_client: NodeClient,
    accounts: Vec<Account>,
    tx_type: TxType,
    target_tps: u32,
    duration_seconds: u64,
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
        target_tps: u32,
        duration_seconds: u64,
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
            duration_seconds,
            use_real_transactions,
            min_amount,
            max_amount,
            stats: BenchmarkStats::default(),
            connection_retry_interval: 1800, // Reconnect every 30 minutes by default
            reconnect_after_errors: 5,       // Reconnect after 5 consecutive errors
            error_count: 0,                  // Start with 0 errors
        })
    }

    pub async fn run(&self) -> Result<BenchmarkStats, BenchError> {
        // Channel for collecting transaction status updates
        let (tx_sender, mut tx_receiver) = mpsc::channel::<TransactionStatus>(1000);
        
        // Channel for signaling benchmark completion
        let (done_tx, done_rx) = watch::channel(false);
        let done_rx_clone = done_rx.clone();
        
        // Start timestamp
        let start_time = Instant::now();
        let target_end_time = start_time + Duration::from_secs(self.duration_seconds);
        
        // Calculate time between transactions to achieve target TPS
        let tx_interval_ms = if self.target_tps > 0 {
            1000.0 / self.target_tps as f64
        } else {
            0.0 // Send as fast as possible
        };
        
        // Maximum number of concurrent submissions
        let max_concurrent_submissions = if self.use_real_transactions {
            // Limit concurrency for real transactions to avoid overloading
            if self.target_tps > 10 { 3 } else { 1 }
        } else {
            // For simulated transactions, allow more concurrency
            10
        };
        
        // Stats
        let stats = Arc::new(Mutex::new(BenchmarkStats::default()));
        let stats_clone = stats.clone();
        
        // In-flight transactions (tx_hash -> submission_time)
        let in_flight = Arc::new(Mutex::new(HashMap::new()));
        let in_flight_clone = in_flight.clone();
        
        // Log process start
        info!(
            "Starting benchmark with {} TPS for {} seconds{}",
            self.target_tps,
            self.duration_seconds,
            if self.use_real_transactions { " using real transactions" } else { "" }
        );
        
        // Spawn transaction status collector
        let status_collector_handle = tokio::spawn(async move {
            // Store transactions that need to be checked
            let mut pending_tx_checks: HashMap<String, Instant> = HashMap::new();
            let mut last_progress_log = Instant::now();
            
            // Track completed transactions
            let mut completed_count = 0;
            let mut confirmed_count = 0;
            let mut failed_count = 0;
            let mut timeout_count = 0;
            
            // Maximum wait time before considering benchmark complete
            let max_wait_time = Duration::from_secs(MAX_WAIT_TIME_MINUTES * 60);
            
            loop {
                // Check if benchmark is done
                if *done_rx.borrow() {
                    // Check if we have pending transactions
                    if pending_tx_checks.is_empty() {
                        break;
                    }
                    
                    // Check if we've waited too long
                    if start_time.elapsed() > max_wait_time {
                        warn!("Maximum wait time reached, ending status collector");
                        
                        // Update stats for timed out transactions
                        let mut stats_guard = stats.lock().await;
                        stats_guard.timeouts += pending_tx_checks.len() as u32;
                        
                        // Log the hashes of timed out transactions
                        if !pending_tx_checks.is_empty() {
                            let timed_out_hashes: Vec<String> = pending_tx_checks.keys().cloned().collect();
                            warn!("Timed out transactions: {:?}", timed_out_hashes);
                        }
                        
                        break;
                    }
                }
                
                // Receive transaction status updates
                match tx_receiver.try_recv() {
                    Ok(status) => match status {
                        TransactionStatus::Submitted { hash, timestamp } => {
                            // Add to pending checks
                            pending_tx_checks.insert(hash, timestamp);
                            
                            // Log occasional debug info
                            if pending_tx_checks.len() % 50 == 0 {
                                debug!("Current pending tx count: {}", pending_tx_checks.len());
                            }
                        },
                        TransactionStatus::Confirmed { hash, block_number, timestamp } => {
                            // Remove from pending checks
                            if pending_tx_checks.remove(&hash).is_some() {
                                // Update stats
                                let mut stats_guard = stats.lock().await;
                                let submission_time = stats_guard.submitted_timestamps.get(&hash).cloned();
                                
                                if let Some(submit_time) = submission_time {
                                    let confirmation_duration = timestamp.duration_since(submit_time);
                                    stats_guard.confirmation_times.push(confirmation_duration);
                                    stats_guard.confirmed_blocks.push(block_number);
                                }
                                
                                stats_guard.confirmed += 1;
                                confirmed_count += 1;
                                completed_count += 1;
                            }
                        },
                        TransactionStatus::Failed { hash, error, timestamp } => {
                            // Remove from pending checks
                            if pending_tx_checks.remove(&hash).is_some() {
                                // Update stats
                                let mut stats_guard = stats.lock().await;
                                stats_guard.errors.push(error);
                                stats_guard.failed += 1;
                                failed_count += 1;
                                completed_count += 1;
                            }
                        },
                        TransactionStatus::TimedOut { hash } => {
                            // Remove from pending checks
                            if pending_tx_checks.remove(&hash).is_some() {
                                // Update stats
                                let mut stats_guard = stats.lock().await;
                                stats_guard.timeouts += 1;
                                timeout_count += 1;
                                completed_count += 1;
                            }
                        },
                    },
                    Err(mpsc::error::TryRecvError::Empty) => {
                        // No new status updates, continue checking existing transactions
                    },
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        // Sender has been dropped, exit after checking remaining transactions
                        if *done_rx.borrow() && pending_tx_checks.is_empty() {
                            break;
                        }
                    },
                }
                
                // Calculate elapsed time
                let elapsed = start_time.elapsed();
                
                // Check pending transactions periodically
                let mut tx_to_check = Vec::new();
                let now = Instant::now();
                
                // Collect transactions ready for checking
                for (hash, submit_time) in pending_tx_checks.iter() {
                    // Basic check interval
                    let time_since_submit = now.duration_since(*submit_time);
                    
                    // Start checking after initial delay
                    if time_since_submit.as_millis() > STATUS_CHECK_INITIAL_DELAY_MS as u128 {
                        tx_to_check.push(hash.clone());
                    }
                    
                    // If transaction has been pending too long, mark as timeout
                    if time_since_submit > Duration::from_secs(5 * 60) {
                        let mut stats_guard = stats.lock().await;
                        stats_guard.timeouts += 1;
                        timeout_count += 1;
                        completed_count += 1;
                        
                        let sender = tx_sender.clone();
                        tokio::spawn(async move {
                            if let Err(e) = sender.send(TransactionStatus::TimedOut { hash }).await {
                                error!("Failed to send timeout status: {}", e);
                            }
                        });
                    }
                }
                
                // Check transaction status in small batches to avoid overloading node
                for hash_batch in tx_to_check.chunks(5) {
                    for hash in hash_batch {
                        // Clone necessary values for async block
                        let hash_clone = hash.clone();
                        let client = self.node_client.clone();
                        let tx_status_sender = tx_sender.clone();
                        
                        // Spawn status check task
                        tokio::spawn(async move {
                            match client.check_transaction_status(&hash_clone).await {
                                Ok(Some(block_number)) => {
                                    // Transaction confirmed in block
                                    let status = TransactionStatus::Confirmed {
                                        hash: hash_clone,
                                        block_number,
                                        timestamp: Instant::now(),
                                    };
                                    
                                    if let Err(e) = tx_status_sender.send(status).await {
                                        error!("Failed to send confirmation status: {}", e);
                                    }
                                },
                                Ok(None) => {
                                    // Transaction is pending - no action needed
                                },
                                Err(e) => {
                                    // Error checking transaction
                                    warn!("Error checking tx {}: {:?}", hash_clone, e);
                                    
                                    // After several retries, consider transaction failed
                                    // Only report once per transaction
                                    let status = TransactionStatus::Failed {
                                        hash: hash_clone,
                                        error: format!("Status check error: {:?}", e),
                                        timestamp: Instant::now(),
                                    };
                                    
                                    if let Err(e) = tx_status_sender.send(status).await {
                                        error!("Failed to send error status: {}", e);
                                    }
                                }
                            }
                        });
                    }
                    
                    // Brief pause between batches to avoid overloading node
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                
                // Periodically log progress
                if now.duration_since(last_progress_log) > Duration::from_secs(PROGRESS_LOG_INTERVAL_SECONDS) {
                    info!(
                        "Progress: {}s elapsed, {} submitted, {} completed ({} confirmed, {} failed, {} timeout), {} in-flight",
                        elapsed.as_secs(),
                        completed_count + pending_tx_checks.len(),
                        completed_count,
                        confirmed_count,
                        failed_count,
                        timeout_count,
                        pending_tx_checks.len()
                    );
                    last_progress_log = now;
                }
                
                // Short sleep to avoid tight loop
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            
            info!(
                "Status collector finished: {} completed ({} confirmed, {} failed, {} timeout)",
                completed_count, confirmed_count, failed_count, timeout_count
            );
        });
        
        // Spawn transaction submitter
        let submitter_handle = tokio::spawn(async move {
            let mut next_tx_time = Instant::now();
            let mut tx_count = 0;
            
            while Instant::now() < target_end_time {
                // Check number of in-flight transactions
                let in_flight_count = in_flight_clone.lock().await.len();
                
                if in_flight_count >= max_concurrent_submissions {
                    // Too many in-flight transactions, wait briefly before checking again
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
                
                // Wait until next transaction time if needed
                let now = Instant::now();
                if tx_interval_ms > 0.0 && now < next_tx_time {
                    tokio::time::sleep(next_tx_time.duration_since(now)).await;
                }
                
                // Get account for transaction
                // Use modulo to cycle through accounts
                let account_idx = tx_count % self.accounts.len();
                let account = &self.accounts[account_idx];
                
                // Submit transaction based on type
                let tx_hash = match self.tx_type {
                    TxType::Transfer => {
                        // For transfer, we need a recipient
                        let recipient = if self.use_real_transactions {
                            // With real transactions, use a predefined recipient
                            // Define a consistent recipient here or get from config
                            "0x8eaf04151687736326c9fea17e25fc5287613693".to_string()
                        } else {
                            // For simulated transactions, generate a random recipient
                            format!("0x{}", hex::encode([0u8; 20]))
                        };
                        
                        self.node_client.submit_transfer(
                            &account.address,
                            &recipient,
                            "0x1000000000000000" // Fixed amount for testing
                        ).await
                    },
                    TxType::Erc20Transfer => {
                        // ERC20 transfer needs recipient and contract address
                        let recipient = "0x8eaf04151687736326c9fea17e25fc5287613693".to_string();
                        let contract = "0xB8c77482e45F1F44dE1745F52C74426C631bDD52".to_string(); // Example contract
                        
                        self.node_client.submit_erc20_transfer(
                            &account.address,
                            &recipient,
                            "0x1000000000000000", // Fixed amount for testing
                            &contract
                        ).await
                    },
                    TxType::ComplexContract => {
                        // Complex contract call
                        let contract = "0xB8c77482e45F1F44dE1745F52C74426C631bDD52".to_string(); // Example contract
                        let data = "0xa9059cbb0000000000000000000000008eaf04151687736326c9fea17e25fc52876136930000000000000000000000000000000000000000000000000000000000000001";
                        
                        self.node_client.submit_complex_contract(
                            &account.address,
                            &contract,
                            data
                        ).await
                    },
                };
                
                // Update stats based on submission result
                match tx_hash {
                    Ok(hash) => {
                        let timestamp = Instant::now();
                        
                        // Add to in-flight transactions
                        in_flight_clone.lock().await.insert(hash.clone(), timestamp);
                        
                        // Update stats
                        let mut stats_guard = stats_clone.lock().await;
                        stats_guard.submitted += 1;
                        stats_guard.submitted_timestamps.insert(hash.clone(), timestamp);
                        
                        // Send status update
                        let status = TransactionStatus::Submitted {
                            hash: hash.clone(),
                            timestamp,
                        };
                        
                        if let Err(e) = tx_sender.send(status).await {
                            error!("Failed to send submission status: {}", e);
                        }
                        
                        // Log occasional transaction submissions
                        if tx_count % 10 == 0 {
                            debug!("Submitted transaction {}: {}", tx_count, hash);
                        }
                    },
                    Err(e) => {
                        // Submission failed, update stats
                        let mut stats_guard = stats_clone.lock().await;
                        stats_guard.errors.push(format!("Submission error: {:?}", e));
                        stats_guard.failed += 1;
                        
                        // Log error
                        error!("Failed to submit transaction {}: {:?}", tx_count, e);
                        
                        // Brief pause before retry to avoid hammering node with errors
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    },
                }
                
                // Increment transaction count
                tx_count += 1;
                
                // Calculate next transaction time
                if tx_interval_ms > 0.0 {
                    next_tx_time = start_time + Duration::from_millis((tx_count as f64 * tx_interval_ms) as u64);
                }
            }
            
            // Signal benchmark completion
            let _ = done_tx.send(true);
            
            info!("Submission phase complete, submitted {} transactions", tx_count);
        });
        
        // Wait for benchmark duration plus a grace period for confirmations
        let benchmark_completion_time = target_end_time + Duration::from_secs(30);
        let timeout_duration = Duration::from_secs((self.duration_seconds + 30).min(7200)); // Max 2 hours
        
        // Wait for benchmark to complete with timeout
        match tokio::time::timeout(timeout_duration, submitter_handle).await {
            Ok(result) => {
                if let Err(e) = result {
                    error!("Submitter task failed: {:?}", e);
                }
            },
            Err(_) => {
                error!("Submitter task timed out after {} seconds", timeout_duration.as_secs());
                // Signal completion anyway to allow status collector to finish
                let _ = done_rx_clone.send(true);
            },
        }
        
        // Wait for status collector with timeout to ensure it doesn't hang
        let status_timeout = Duration::from_secs(60); // 60 second timeout
        match tokio::time::timeout(status_timeout, status_collector_handle).await {
            Ok(result) => {
                if let Err(e) = result {
                    error!("Status collector task failed: {:?}", e);
                }
            },
            Err(_) => {
                error!("Status collector task timed out");
            },
        }
        
        // Return final stats
        let stats = stats_clone.lock().await.clone();
        
        // Log final results
        info!("Benchmark complete: {} submitted, {} confirmed, {} failed, {} timed out", 
            stats.submitted, stats.confirmed, stats.failed, stats.timeouts);
        
        if !stats.confirmation_times.is_empty() {
            // Calculate average confirmation time
            let total_time: Duration = stats.confirmation_times.iter().sum();
            let avg_time = total_time / stats.confirmation_times.len() as u32;
            
            info!("Average confirmation time: {}ms", avg_time.as_millis());
        }
        
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