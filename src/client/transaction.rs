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
    // Use predefined addresses for deterministic testing
    let recipients = vec![
        "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY", // Alice
        "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty", // Bob
        "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y", // Charlie
        "5DAAnrj7VHTznn2AWBemMuyBwZWs6FNFjdyVXUeYum3PTXFy", // Dave
        "5HGjWAeFDfFCWPsjFQdVV2Msvz2XtMktvgocEZcCj68kUMaw", // Eve
    ];
    
    let mut rng = thread_rng();
    let index = rng.gen_range(0..recipients.len());
    recipients[index].to_string()
} 