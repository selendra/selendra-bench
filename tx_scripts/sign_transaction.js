
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
