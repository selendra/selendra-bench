
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
