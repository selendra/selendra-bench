#!/bin/bash
cd tx_scripts
npm install
node sign_transaction.js "$@"
