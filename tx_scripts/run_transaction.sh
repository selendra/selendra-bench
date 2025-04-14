#!/bin/bash
cd "/home/user0/projects/selendra-bench/tx_scripts"
if [ ! -d "node_modules" ]; then
    echo "Installing dependencies..."
    npm install
fi
echo "Running transaction script..."
node sign_transaction.js "$@"
