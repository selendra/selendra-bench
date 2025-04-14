use std::error::Error;
use std::process::Command;
use serde_json::{json, Value};
use crate::types::Account;
use std::path::Path;
use std::io::Write;
use rand::{thread_rng, Rng};
use tokio::process::Command as AsyncCommand;
use crate::client::BenchError;
use std::fs::File;
use std::fs;
use tokio::time::{sleep, Duration};

const MAX_RETRIES: usize = 3;
const RETRY_DELAY_MS: u64 = 500;

// This function will create a shell script that uses polkadot-js API to sign and send a transaction
pub async fn create_transaction_script(tx_script_dir: &str) -> Result<(), BenchError> {
    // Make sure the directory exists with proper permissions
    println!("Creating transaction script directory: {}", tx_script_dir);
    fs::create_dir_all(tx_script_dir)
        .map_err(|e| BenchError::IOError(format!("Failed to create script directory: {}", e)))?;
    
    // Create the package.json file for Node.js dependencies
    let package_json = r#"{
  "name": "selendra-transaction-signer",
  "version": "1.0.0",
  "description": "Script to sign transactions for Selendra benchmarking",
  "main": "sign.js",
  "dependencies": {
    "ethers": "^5.7.0",
    "web3": "^1.7.4"
  }
}"#;
    
    let mut file = File::create(Path::new(tx_script_dir).join("package.json"))
        .map_err(|e| BenchError::IOError(format!("Failed to create package.json: {}", e)))?;
    file.write_all(package_json.as_bytes())
        .map_err(|e| BenchError::IOError(format!("Failed to write to package.json: {}", e)))?;
    
    // Create signing script
    let sign_js = r#"const { ethers } = require('ethers');

async function signTransaction() {
    try {
        const privateKey = process.argv[2];
        const recipient = process.argv[3];
        const amount = process.argv[4];
        const nonce = parseInt(process.argv[5]);
        const gasPrice = process.argv[6];
        const gasLimit = process.argv[7];
        const chainId = parseInt(process.argv[8]);
        
        // Create wallet from private key
        const wallet = new ethers.Wallet(privateKey);
        
        // Create transaction object
        const tx = {
            to: recipient,
            value: ethers.utils.parseEther(amount),
            gasLimit: gasLimit,
            gasPrice: ethers.utils.parseUnits(gasPrice, 'gwei'),
            nonce: nonce,
            chainId: chainId
        };
        
        // Sign the transaction
        const signedTx = await wallet.signTransaction(tx);
        
        // Output the signed transaction
        console.log(signedTx);
    } catch (error) {
        console.error(`Error: ${error.message}`);
        process.exit(1);
    }
}

signTransaction();
"#;
    
    let mut file = File::create(Path::new(tx_script_dir).join("sign.js"))
        .map_err(|e| BenchError::IOError(format!("Failed to create sign.js: {}", e)))?;
    file.write_all(sign_js.as_bytes())
        .map_err(|e| BenchError::IOError(format!("Failed to write to sign.js: {}", e)))?;
    
    // Create run script
    let run_sh = r#"#!/bin/bash
# Script to sign and send a transaction
# Usage: ./run.sh private_key recipient amount nonce gas_price gas_limit chain_id rpc_url

NODE_PATH=$(npm root -g)
node sign.js "$1" "$2" "$3" "$4" "$5" "$6" "$7" | xargs -I{} curl -X POST -H "Content-Type: application/json" --data "{\"jsonrpc\":\"2.0\",\"method\":\"eth_sendRawTransaction\",\"params\":[\"{}\"],\"id\":1}" "$8"
"#;
    
    let mut file = File::create(Path::new(tx_script_dir).join("run.sh"))
        .map_err(|e| BenchError::IOError(format!("Failed to create run.sh: {}", e)))?;
    file.write_all(run_sh.as_bytes())
        .map_err(|e| BenchError::IOError(format!("Failed to write to run.sh: {}", e)))?;
    
    // Make run.sh executable
    let status = std::process::Command::new("chmod")
        .arg("+x")
        .arg(Path::new(tx_script_dir).join("run.sh"))
        .status()
        .map_err(|e| BenchError::IOError(format!("Failed to make run.sh executable: {}", e)))?;
    
    if !status.success() {
        return Err(BenchError::IOError("Failed to make run.sh executable".to_string()));
    }
    
    // Install dependencies
    let status = std::process::Command::new("npm")
        .arg("install")
        .current_dir(tx_script_dir)
        .status()
        .map_err(|e| BenchError::IOError(format!("Failed to install npm dependencies: {}", e)))?;
    
    if !status.success() {
        return Err(BenchError::IOError("Failed to install npm dependencies".to_string()));
    }
    
    Ok(())
}

// Generate a random recipient address for testing
pub fn generate_random_recipient() -> String {
    // List of fixed recipients for deterministic testing
    let recipients = vec![
        "0x8097c3C354652CB1EEed3E5B65fBa2576470678A",
        "0x5Bd7FC0D131070866836BdDE07fB9E5DDDA39C40",
        "0x9A676e781A523b5d0C0e43731313A708CB607508",
        "0x8E4340a141d1D5B30A60E38053A5D48355A4479C",
        "0x512fE6BcA7e06dF2f9CFD4d9Ef4B8F9DBe74F394",
        "0xA6f79B60359f141df90A0C745125B131cAAfFD12",
        "0xaEEF3B8A506774cDD467fAa5F6D257d001E5Db97",
        "0x5F51DC553F7A8CF3cc397640A7bC96bf3BB8f0E6",
        "0x1Ef1BC8c613f5F1d59A0F47BFB21685AA66804ef",
        "0x3Cb0DF8A2655b8B1693e27c4D76B68db857c62D4"
    ];
    
    let index = thread_rng().gen_range(0..recipients.len());
    recipients[index].to_string()
}

pub async fn submit_real_transaction(
    private_key: &str,
    recipient: &str,
    amount: &str,
    nonce: u32,
    gas_price: &str,
    gas_limit: &str,
    chain_id: u32,
    rpc_url: &str,
    tx_script_dir: &str
) -> Result<String, BenchError> {
    let mut retries = 0;
    let mut last_error = None;
    
    while retries < MAX_RETRIES {
        if retries > 0 {
            println!("Retrying transaction submission (attempt {}/{})", retries + 1, MAX_RETRIES);
            sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
        }
        
        // Run the transaction script
        match AsyncCommand::new("sh")
            .arg("-c")
            .arg(format!(
                "cd {} && ./run.sh '{}' '{}' '{}' {} '{}' '{}' {} '{}'",
                tx_script_dir, private_key, recipient, amount, nonce, gas_price, gas_limit, chain_id, rpc_url
            ))
            .output()
            .await {
                Ok(output) => {
                    // Process the output
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        
                        // Parse JSON response to extract transaction hash
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                            if let Some(result) = json.get("result") {
                                if let Some(tx_hash) = result.as_str() {
                                    return Ok(tx_hash.to_string());
                                }
                            }
                            
                            // Check for error
                            if let Some(error) = json.get("error") {
                                if let Some(message) = error.get("message").and_then(|m| m.as_str()) {
                                    if message.contains("nonce too low") || 
                                       message.contains("replacement transaction underpriced") {
                                        // If nonce issue, increment and retry immediately
                                        println!("Nonce issue detected: {}", message);
                                        let _ = MAX_RETRIES; // Force exit loop
                                        return Err(BenchError::NonceError(message.to_string()));
                                    } else {
                                        last_error = Some(BenchError::RPCError(message.to_string()));
                                    }
                                } else {
                                    last_error = Some(BenchError::RPCError(format!("Unknown RPC error: {:?}", error)));
                                }
                            } else {
                                last_error = Some(BenchError::RPCError(format!("Missing result or error in response: {}", stdout)));
                            }
                        } else {
                            // Not valid JSON, could be curl error or other issue
                            last_error = Some(BenchError::ParseError(format!("Invalid JSON response: {}", stdout)));
                        }
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        last_error = Some(BenchError::CommandError(format!("Command failed: {}", stderr)));
                    }
                },
                Err(e) => {
                    last_error = Some(BenchError::CommandError(format!("Failed to execute command: {}", e)));
                }
            }
        
        retries += 1;
    }
    
    // All retries failed
    Err(last_error.unwrap_or_else(|| BenchError::CommandError("Transaction submission failed after all retries".to_string())))
} 