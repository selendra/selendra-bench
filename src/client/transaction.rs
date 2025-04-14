use std::error::Error;
use std::process::Command;
use serde_json::{json, Value};
use crate::types::Account;
use std::fs;
use std::path::Path;
use std::io::Write;
use rand::{thread_rng, Rng};
use tokio::process::Command as AsyncCommand;
use crate::client::BenchError;
use std::fs::{self, File};
use tokio::time::{sleep, Duration};

const MAX_RETRIES: usize = 3;
const RETRY_DELAY_MS: u64 = 500;

// This function will create a shell script that uses polkadot-js API to sign and send a transaction
pub fn create_transaction_script(output_dir: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Make sure the directory exists with proper permissions
    println!("Creating transaction script directory: {}", output_dir);
    fs::create_dir_all(output_dir)?;
    
    // Create the package.json file for Node.js dependencies
    let package_json = json!({
        "name": "selendra-transaction-signer",
        "version": "1.0.0",
        "description": "Signs and submits transactions to Selendra Network",
        "main": "sign_transaction.js",
        "dependencies": {
            "@polkadot/api": "^10.9.1",
            "@polkadot/keyring": "^12.5.1",
            "@polkadot/util": "^12.5.1",
            "@polkadot/util-crypto": "^12.5.1"
        }
    });
    
    let package_json_path = format!("{}/package.json", output_dir);
    let mut file = fs::File::create(&package_json_path)?;
    file.write_all(serde_json::to_string_pretty(&package_json)?.as_bytes())?;
    
    // Create the transaction signing script
    let js_script = r#"
const { ApiPromise, WsProvider, Keyring } = require('@polkadot/api');
const { cryptoWaitReady } = require('@polkadot/util-crypto');
const fs = require('fs');
const path = require('path');

// Get command line args
const args = process.argv.slice(2);
if (args.length < 5) {
  console.error('Usage: node sign_transaction.js <endpoint> <seed_phrase> <recipient> <amount> <output_file>');
  process.exit(1);
}

const endpoint = args[0];
const seedPhrase = args[1];
const recipient = args[2];
const amount = BigInt(args[3]);
const outputFile = args[4];

// Ensure directory exists for output file
const outputDir = path.dirname(outputFile);
try {
  if (!fs.existsSync(outputDir)) {
    fs.mkdirSync(outputDir, { recursive: true });
    console.log(`Created directory: ${outputDir}`);
  }
} catch (err) {
  console.error(`Error creating directory: ${err.message}`);
}

console.log(`Starting transaction submission...`);
console.log(`Endpoint: ${endpoint}`);
console.log(`Recipient: ${recipient}`);
console.log(`Amount: ${amount}`);
console.log(`Output file: ${outputFile}`);

// Helper function to write to file with directory creation
function safeWriteFileSync(filePath, data) {
  try {
    const dir = path.dirname(filePath);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
    fs.writeFileSync(filePath, data);
    return true;
  } catch (err) {
    console.error(`Error writing to file ${filePath}: ${err.message}`);
    return false;
  }
}

async function main() {
  try {
    console.log('Waiting for crypto to be ready...');
    // Wait for the crypto to be ready
    await cryptoWaitReady();
    
    console.log('Connecting to node...');
    // Connect to the node
    const wsProvider = new WsProvider(endpoint);
    const api = await ApiPromise.create({ provider: wsProvider });
    
    // Write initial status to file
    const initStatus = {
      success: false,
      tx_hash: "pending",
      status: "connecting"
    };
    safeWriteFileSync(outputFile, JSON.stringify(initStatus, null, 2));
    
    // Get API methods
    console.log('Checking available methods...');
    const modules = Object.keys(api.tx);
    console.log(`Available modules: ${modules.join(', ')}`);
    
    // Try to find the right transfer method
    let transferMethod;
    let transferModule;
    
    if (api.tx.balances && typeof api.tx.balances.transfer === 'function') {
      transferMethod = api.tx.balances.transfer;
      transferModule = 'balances.transfer';
    } else if (api.tx.balances && typeof api.tx.balances.transferAllowDeath === 'function') {
      transferMethod = api.tx.balances.transferAllowDeath;
      transferModule = 'balances.transferAllowDeath';
    } else if (api.tx.balances && typeof api.tx.balances.transferKeepAlive === 'function') {
      transferMethod = api.tx.balances.transferKeepAlive;
      transferModule = 'balances.transferKeepAlive';
    } else if (api.tx.balances && typeof api.tx.balances.transferAll === 'function') {
      transferMethod = api.tx.balances.transferAll;
      transferModule = 'balances.transferAll';
    } else if (api.tx.currencies && typeof api.tx.currencies.transfer === 'function') {
      transferMethod = api.tx.currencies.transfer;
      transferModule = 'currencies.transfer';
    } else if (api.tx.tokens && typeof api.tx.tokens.transfer === 'function') {
      transferMethod = api.tx.tokens.transfer;
      transferModule = 'tokens.transfer';
    } else if (api.tx.selendra && typeof api.tx.selendra.transfer === 'function') {
      transferMethod = api.tx.selendra.transfer;
      transferModule = 'selendra.transfer';
    } else if (api.tx.indices && typeof api.tx.indices.transfer === 'function') {
      transferMethod = api.tx.indices.transfer;
      transferModule = 'indices.transfer';
    } else {
      // Try to dump all available methods
      let availableMethods = [];
      for (const mod of modules) {
        try {
          const methods = Object.keys(api.tx[mod]);
          availableMethods.push(`${mod}: ${methods.join(', ')}`);
        } catch (e) {
          availableMethods.push(`${mod}: [error getting methods]`);
        }
      }
      
      const errorMsg = `Could not find a suitable transfer method. Available methods:\n${availableMethods.join('\n')}`;
      console.error(errorMsg);
      
      const errorResult = {
        success: false,
        error: errorMsg,
        modules: modules
      };
      safeWriteFileSync(outputFile, JSON.stringify(errorResult, null, 2));
      await api.disconnect();
      process.exit(1);
    }
    
    console.log(`Using transfer method: ${transferModule}`);
    
    console.log('Creating account from seed phrase...');
    // Get the account from the seed phrase
    const keyring = new Keyring({ type: 'sr25519' });
    const account = keyring.addFromUri(seedPhrase);
    
    console.log(`Sender: ${account.address}`);
    console.log(`Recipient: ${recipient}`);
    console.log(`Amount: ${amount}`);
    
    // Update status
    const preparingStatus = {
      success: false,
      tx_hash: "pending",
      sender: account.address,
      recipient: recipient,
      amount: amount.toString(),
      status: "preparing"
    };
    safeWriteFileSync(outputFile, JSON.stringify(preparingStatus, null, 2));
    
    try {
      console.log('Creating and signing transaction...');
      // Create a transfer transaction
      const unsub = await transferMethod(recipient, amount)
        .signAndSend(account, (result) => {
          console.log(`Current transaction status: ${result.status.type}`);
          
          if (result.status.isInBlock) {
            console.log(`Transaction included in block: ${result.status.asInBlock.toString()}`);
            
            // Write the tx hash to the output file
            const txResult = {
              success: true,
              tx_hash: result.txHash.toString(),
              sender: account.address,
              recipient: recipient,
              amount: amount.toString(),
              block: result.status.asInBlock.toString(),
              events: result.events.map(e => ({
                section: e.event.section,
                method: e.event.method,
                data: e.event.data.toString()
              }))
            };
            
            safeWriteFileSync(outputFile, JSON.stringify(txResult, null, 2));
            console.log(`Transaction included with hash: ${result.txHash.toString()}`);
            
            if (unsub) {
              unsub();
              setTimeout(() => {
                api.disconnect();
                process.exit(0);
              }, 5000);
            }
          } else if (result.status.isFinalized) {
            console.log(`Transaction finalized in block: ${result.status.asFinalized.toString()}`);
            
            // Update the tx hash info
            if (fs.existsSync(outputFile)) {
              try {
                const existing = JSON.parse(fs.readFileSync(outputFile, 'utf8'));
                existing.finalized = true;
                existing.finalizedBlock = result.status.asFinalized.toString();
                safeWriteFileSync(outputFile, JSON.stringify(existing, null, 2));
              } catch (err) {
                console.error(`Error updating output file: ${err.message}`);
              }
            }
            
            if (unsub) {
              unsub();
              setTimeout(() => {
                api.disconnect();
                process.exit(0);
              }, 2000);
            }
          } else if (result.status.isError) {
            console.error(`Transaction error: ${JSON.stringify(result)}`);
            const errorResult = {
              success: false,
              error: `Transaction error: ${result.status.type}`,
              details: JSON.stringify(result)
            };
            safeWriteFileSync(outputFile, JSON.stringify(errorResult, null, 2));
            
            if (unsub) {
              unsub();
              setTimeout(() => {
                api.disconnect();
                process.exit(1);
              }, 1000);
            }
          }
        });
        
      // Write initial status
      const initStatus = {
        success: true,
        tx_hash: "pending",
        sender: account.address,
        recipient: recipient,
        amount: amount.toString(),
        status: "submitted"
      };
      safeWriteFileSync(outputFile, JSON.stringify(initStatus, null, 2));
      
      // Keep process alive until callback completes
      console.log('Transaction submitted, waiting for inclusion...');
      
    } catch (error) {
      console.error(`Error submitting transaction: ${error.message}`);
      console.error(error.stack);
      const result = {
        success: false,
        error: error.toString(),
        stack: error.stack
      };
      
      safeWriteFileSync(outputFile, JSON.stringify(result, null, 2));
      await api.disconnect();
      process.exit(1);
    }
  } catch (error) {
    console.error(`Fatal error: ${error.message}`);
    console.error(error.stack);
    const result = {
      success: false,
      error: error.toString(),
      stack: error.stack
    };
    
    safeWriteFileSync(outputFile, JSON.stringify(result, null, 2));
    process.exit(1);
  }
}

// Set a timeout to avoid hanging forever
const timeout = setTimeout(() => {
  console.error('Script timed out after 120 seconds');
  const result = {
    success: false,
    error: 'Timeout waiting for transaction'
  };
  safeWriteFileSync(outputFile, JSON.stringify(result, null, 2));
  process.exit(1);
}, 120000);  // Increase timeout to 120 seconds

main().catch(error => {
  console.error(`Unhandled error: ${error.message}`);
  console.error(error.stack);
  const result = {
    success: false,
    error: error.toString(),
    stack: error.stack
  };
  
  safeWriteFileSync(outputFile, JSON.stringify(result, null, 2));
  clearTimeout(timeout);
  process.exit(1);
});
"#;
    
    let js_script_path = format!("{}/sign_transaction.js", output_dir);
    let mut file = fs::File::create(&js_script_path)?;
    file.write_all(js_script.as_bytes())?;
    
    // Create a shell script to run the Node.js script
    let shell_script = format!(
        r#"#!/bin/bash
cd "{}"
if [ ! -d "node_modules" ]; then
    echo "Installing dependencies..."
    npm install
fi
echo "Running transaction script..."
node sign_transaction.js "$@"
"#, 
        output_dir
    );
    
    let shell_script_path = format!("{}/run_transaction.sh", output_dir);
    let mut file = fs::File::create(&shell_script_path)?;
    file.write_all(shell_script.as_bytes())?;
    
    // Make the shell script executable
    Command::new("chmod")
        .args(["+x", &shell_script_path])
        .output()?;
    
    Ok(())
}

// Function to submit a real transaction using the Node.js bridge
pub async fn submit_real_transaction(
    endpoint: &str,
    seed_phrase: &str,
    recipient: &str,
    amount: u128,
    tx_script_dir: &str
) -> Result<String, Box<dyn Error + Send + Sync>> {
    // Path to the shell script
    let script_path = format!("{}/run_transaction.sh", tx_script_dir);
    
    // Output file for the transaction result
    let output_file = format!("{}/tx_result_{}.json", tx_script_dir, chrono::Utc::now().timestamp());
    
    // Run the script to sign and submit the transaction
    let output = Command::new(&script_path)
        .args([
            endpoint,
            seed_phrase,
            recipient,
            &amount.to_string(),
            &output_file,
        ])
        .output()?;
    
    if !output.status.success() {
        return Err(format!("Transaction script failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    
    // Read the transaction result
    if Path::new(&output_file).exists() {
        let result_str = fs::read_to_string(&output_file)?;
        let result: Value = serde_json::from_str(&result_str)?;
        
        if result["success"].as_bool().unwrap_or(false) {
            Ok(result["tx_hash"].as_str().unwrap_or("unknown").to_string())
        } else {
            Err(format!("Transaction failed: {}", result["error"].as_str().unwrap_or("Unknown error")).into())
        }
    } else {
        Err("Transaction output file not found".into())
    }
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

pub async fn create_transaction_script(tx_script_dir: &str) -> Result<(), BenchError> {
    // Create directory if it doesn't exist
    fs::create_dir_all(tx_script_dir)
        .map_err(|e| BenchError::IOError(format!("Failed to create script directory: {}", e)))?;
    
    // Create package.json
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
                                        retries = MAX_RETRIES; // Force exit loop
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