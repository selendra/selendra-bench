#!/bin/bash
# Selendra Network TPS Benchmarking Script with Real Transactions
# This script automates TPS benchmarking for Selendra Network using real transactions
# Usage: ./benchmark-blocksize.sh --node-url wss://rpcx.selendra.org --seed-phrase "your seed phrase" --use-real-transactions

set -e

# Default values
NODE_URL="wss://rpcx.selendra.org"
DURATION=10
OUTPUT_DIR="benchmark_results/$(date +%Y%m%d_%H%M%S)"
TPS_LEVELS=(1 2 3 5 8)
TX_TYPE="transfer"
NUM_ACCOUNTS=5
USE_REAL_TRANSACTIONS=false
SEED_PHRASE=""
MIN_AMOUNT=1000000  # 0.001 SEL (assuming 6 decimals)
MAX_AMOUNT=5000000  # 0.005 SEL

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --node-url)
      NODE_URL="$2"
      shift 2
      ;;
    --duration)
      DURATION="$2"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="$2"
      shift 2
      ;;
    --tps)
      IFS=',' read -ra TPS_LEVELS <<< "$2"
      shift 2
      ;;
    --tx-type)
      TX_TYPE="$2"
      shift 2
      ;;
    --accounts)
      NUM_ACCOUNTS="$2"
      shift 2
      ;;
    --use-real-transactions)
      USE_REAL_TRANSACTIONS=true
      shift
      ;;
    --seed-phrase)
      SEED_PHRASE="$2"
      shift 2
      ;;
    --min-amount)
      MIN_AMOUNT="$2"
      shift 2
      ;;
    --max-amount)
      MAX_AMOUNT="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

# Check if seed phrase is provided for real transactions
if [ "$USE_REAL_TRANSACTIONS" = true ] && [ -z "$SEED_PHRASE" ]; then
  echo "Error: You must provide a seed phrase with --seed-phrase when using real transactions"
  exit 1
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Function to run a single benchmark
run_benchmark() {
  local tps=$1
  local output_file="$OUTPUT_DIR/benchmark_${TX_TYPE}_${tps}tps.json"
  
  echo "========================================================"
  echo "Running benchmark with ${TX_TYPE} transactions at ${tps} TPS"
  echo "Node URL: $NODE_URL"
  echo "Accounts: $NUM_ACCOUNTS"
  echo "Duration: $DURATION seconds"
  if [ "$USE_REAL_TRANSACTIONS" = true ]; then
    echo "Using REAL transactions"
    echo "Amount range: $MIN_AMOUNT to $MAX_AMOUNT (smallest units)"
  else
    echo "Using simulated transactions"
  fi
  echo "Output will be saved to: $output_file"
  echo "========================================================"
  
  # Build the command
  cmd="./target/release/selendra-bench \
    --node-url \"$NODE_URL\" \
    --tx-type \"$TX_TYPE\" \
    --accounts \"$NUM_ACCOUNTS\" \
    --tps-target \"$tps\" \
    --duration \"$DURATION\" \
    --output \"$output_file\""
  
  # Add real transaction options if enabled
  if [ "$USE_REAL_TRANSACTIONS" = true ]; then
    cmd="$cmd \
    --real-transactions \
    --seed-phrase \"$SEED_PHRASE\" \
    --min-amount \"$MIN_AMOUNT\" \
    --max-amount \"$MAX_AMOUNT\""
  fi
  
  # Run the benchmark
  echo "Starting benchmark..."
  echo "Command: $cmd"
  
  if [ "$USE_REAL_TRANSACTIONS" = true ]; then
    # For real transactions, use eval to preserve quotes in the seed phrase
    eval "$cmd" || {
      echo "Benchmark failed! See output above for details."
      echo "Continuing with next benchmark..."
      echo ""
      return 1
    }
  else
    # For simulated transactions, just run the command directly
    ./target/release/selendra-bench \
      --node-url "$NODE_URL" \
      --tx-type "$TX_TYPE" \
      --accounts "$NUM_ACCOUNTS" \
      --tps-target "$tps" \
      --duration "$DURATION" \
      --output "$output_file" || {
        echo "Benchmark failed! See output above for details."
        echo "Continuing with next benchmark..."
        echo ""
        return 1
      }
  fi
  
  echo "Benchmark complete. Results saved to $output_file"
  echo ""
  
  # Wait between benchmarks to avoid overwhelming the node
  # Use longer wait times for real transactions
  if [ "$USE_REAL_TRANSACTIONS" = true ]; then
    echo "Waiting 30 seconds before next benchmark to allow transactions to settle..."
    sleep 30
  else
    echo "Waiting 5 seconds before next benchmark..."
    sleep 5
  fi
}

# Write benchmark configuration to a file
cat > "$OUTPUT_DIR/benchmark_config.txt" << EOF
Selendra Network TPS Benchmark
==============================
Date: $(date)
Node URL: $NODE_URL
Transaction Type: $TX_TYPE
Number of Accounts: $NUM_ACCOUNTS
Duration per Test: $DURATION seconds
TPS Levels Tested: ${TPS_LEVELS[@]}
Using Real Transactions: $USE_REAL_TRANSACTIONS
EOF

# Add min/max amount if using real transactions
if [ "$USE_REAL_TRANSACTIONS" = true ]; then
  cat >> "$OUTPUT_DIR/benchmark_config.txt" << EOF
Min Amount: $MIN_AMOUNT
Max Amount: $MAX_AMOUNT
EOF
fi

# Run benchmarks for each TPS level
echo "Starting Selendra Network TPS benchmarking"
echo "==========================================="
echo "Configuration:"
echo "  Node URL: $NODE_URL"
echo "  Transaction Type: $TX_TYPE"
echo "  Number of Accounts: $NUM_ACCOUNTS"
echo "  Duration per Test: $DURATION seconds"
echo "  TPS Levels: ${TPS_LEVELS[@]}"
echo "  Using Real Transactions: $USE_REAL_TRANSACTIONS"
if [ "$USE_REAL_TRANSACTIONS" = true ]; then
  echo "  Amount Range: $MIN_AMOUNT to $MAX_AMOUNT (smallest units)"
fi
echo "  Results Directory: $OUTPUT_DIR"
echo ""

for tps in "${TPS_LEVELS[@]}"; do
  run_benchmark "$tps" || true
done

echo "TPS benchmarking completed!"
echo "Results saved in: $OUTPUT_DIR/"

# Check if we have any result files
if ls "$OUTPUT_DIR/benchmark_"*".json" 1> /dev/null 2>&1; then
  echo "Summary of results:"
  for result in "$OUTPUT_DIR/benchmark_"*".json"; do
    # Extract TPS from filename
    filename=$(basename "$result")
    tps=$(echo "$filename" | sed -E 's/.*_([0-9]+)tps\.json/\1/')
    
    # Parse success rate from JSON file
    if [ -s "$result" ]; then
      submitted=$(grep -o '"submitted": [0-9]*' "$result" | awk '{print $2}')
      successful=$(grep -o '"successful": [0-9]*' "$result" | awk '{print $2}')
      failed=$(grep -o '"failed": [0-9]*' "$result" | awk '{print $2}')
      
      if [ -n "$submitted" ] && [ "$submitted" -ne 0 ]; then
        success_rate=$(awk "BEGIN { printf \"%.1f\", ($successful / $submitted) * 100 }")
        echo "  TPS $tps: Submitted=$submitted, Successful=$successful, Failed=$failed, Success Rate=${success_rate}%"
      else
        echo "  TPS $tps: No data or invalid file format"
      fi
    else
      echo "  TPS $tps: Empty result file"
    fi
  done > "$OUTPUT_DIR/summary.txt"
  
  # Show the summary
  cat "$OUTPUT_DIR/summary.txt"
else
  echo "No result files were generated. All benchmarks may have failed."
fi

# Generate a recommendations section
cat > "$OUTPUT_DIR/recommendations.txt" << EOF
Selendra Network TPS Benchmark Recommendations
=============================================

Based on the benchmark results, here are the TPS recommendations:

EOF

# Check if we have a summary file
if [ -f "$OUTPUT_DIR/summary.txt" ]; then
  # Find the highest TPS with a success rate > 95%
  high_success_tps=$(grep -E "Success Rate=9[5-9].[0-9]%" "$OUTPUT_DIR/summary.txt" | sort -k2,2rn | head -1 | sed -E 's/.*TPS ([0-9]+).*/\1/' || echo "Unknown")
  
  # Find the highest TPS with a success rate > 80%
  medium_success_tps=$(grep -E "Success Rate=8[0-9].[0-9]%" "$OUTPUT_DIR/summary.txt" | sort -k2,2rn | head -1 | sed -E 's/.*TPS ([0-9]+).*/\1/' || echo "Unknown")
  
  cat >> "$OUTPUT_DIR/recommendations.txt" << EOF
1. Optimal TPS (>95% success rate): $high_success_tps
2. Maximum Safe TPS (>80% success rate): $medium_success_tps

For production use, we recommend staying at or below the Optimal TPS value
to ensure consistent transaction processing. The Maximum Safe TPS can be
used for short bursts of activity but may result in higher failure rates.

EOF
else
  cat >> "$OUTPUT_DIR/recommendations.txt" << EOF
No recommendations available - benchmarks did not complete successfully.
EOF
fi

echo ""
echo "Recommendations have been saved to: $OUTPUT_DIR/recommendations.txt"