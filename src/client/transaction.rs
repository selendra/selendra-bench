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
    // Make sure the directory exists
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

async function main() {
  // Wait for the crypto to be ready
  await cryptoWaitReady();
  
  // Connect to the node
  const wsProvider = new WsProvider(endpoint);
  const api = await ApiPromise.create({ provider: wsProvider });
  
  // Get the account from the seed phrase
  const keyring = new Keyring({ type: 'sr25519' });
  const account = keyring.addFromUri(seedPhrase);
  
  console.log(`Sender: ${account.address}`);
  console.log(`Recipient: ${recipient}`);
  console.log(`Amount: ${amount}`);
  
  try {
    // Create a transfer transaction
    const txHash = await api.tx.balances
      .transfer(recipient, amount)
      .signAndSend(account);
      
    // Write the tx hash to the output file
    const result = {
      success: true,
      tx_hash: txHash.toString(),
      sender: account.address,
      recipient: recipient,
      amount: amount.toString()
    };
    
    fs.writeFileSync(outputFile, JSON.stringify(result, null, 2));
    console.log(`Transaction submitted with hash: ${txHash.toString()}`);
  } catch (error) {
    const result = {
      success: false,
      error: error.toString()
    };
    
    fs.writeFileSync(outputFile, JSON.stringify(result, null, 2));
    console.error(`Error submitting transaction: ${error}`);
  }
  
  // Disconnect from the node
  await api.disconnect();
}

main().catch(error => {
  console.error(`Fatal error: ${error}`);
  process.exit(1);
});
"#;
    
    let js_script_path = format!("{}/sign_transaction.js", output_dir);
    let mut file = fs::File::create(&js_script_path)?;
    file.write_all(js_script.as_bytes())?;
    
    // Create a shell script to run the Node.js script
    let shell_script = format!(
        r#"#!/bin/bash
cd {}
npm install
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