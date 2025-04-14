#!/bin/bash

# Default values
NODE_URL="wss://rpcx.selendra.org"
BACKUP_URL="wss://rpc.selendra.org"
DURATION=120  # 2 minutes per test
MIN_AMOUNT=1000000
MAX_AMOUNT=5000000
ACCOUNTS=5
OUTPUT_DIR="tps_test_results"
START_TPS=1
MAX_TPS=10
STEP=1
SEED_PHRASE=""

# Display usage information
function show_usage() {
  echo "Usage: $0 --seed-phrase \"your seed phrase\" [OPTIONS]"
  echo ""
  echo "Required:"
  echo "  --seed-phrase \"PHRASE\"   Seed phrase for the account (required)"
  echo ""
  echo "Options:"
  echo "  --node-url URL           Primary node URL (default: wss://rpcx.selendra.org)"
  echo "  --backup-url URL         Backup node URL (default: wss://rpc.selendra.org)"
  echo "  --duration SECONDS       Duration of each test in seconds (default: 120)"
  echo "  --min-amount AMOUNT      Minimum transaction amount (default: 1000000)"
  echo "  --max-amount AMOUNT      Maximum transaction amount (default: 5000000)"
  echo "  --accounts NUMBER        Number of accounts to use (default: 5)"
  echo "  --output-dir DIRECTORY   Output directory (default: tps_test_results)"
  echo "  --start-tps NUMBER       Starting TPS value (default: 1)"
  echo "  --max-tps NUMBER         Maximum TPS to test (default: 10)"
  echo "  --step NUMBER            TPS increment step (default: 1)"
  echo "  --help                   Display this help message"
  exit 1
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
    --seed-phrase)
      SEED_PHRASE="$2"
      shift 2
      ;;
    --node-url)
      NODE_URL="$2"
      shift 2
      ;;
    --backup-url)
      BACKUP_URL="$2"
      shift 2
      ;;
    --duration)
      DURATION="$2"
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
    --accounts)
      ACCOUNTS="$2"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="$2"
      shift 2
      ;;
    --start-tps)
      START_TPS="$2"
      shift 2
      ;;
    --max-tps)
      MAX_TPS="$2"
      shift 2
      ;;
    --step)
      STEP="$2"
      shift 2
      ;;
    --help)
      show_usage
      ;;
    *)
      echo "Unknown option: $1"
      show_usage
      ;;
  esac
done

# Check required arguments
if [ -z "$SEED_PHRASE" ]; then
  echo "Error: Seed phrase is required"
  show_usage
fi

mkdir -p "$OUTPUT_DIR"

echo "TPS Test Configuration" > "$OUTPUT_DIR/config.txt"
echo "Node URL: $NODE_URL" >> "$OUTPUT_DIR/config.txt"
echo "Backup URL: $BACKUP_URL" >> "$OUTPUT_DIR/config.txt"
echo "Duration per test: $DURATION seconds" >> "$OUTPUT_DIR/config.txt"
echo "Accounts: $ACCOUNTS" >> "$OUTPUT_DIR/config.txt"
echo "Amount range: $MIN_AMOUNT to $MAX_AMOUNT" >> "$OUTPUT_DIR/config.txt"
echo "Starting TPS: $START_TPS" >> "$OUTPUT_DIR/config.txt"
echo "Maximum TPS: $MAX_TPS" >> "$OUTPUT_DIR/config.txt"
echo "TPS increment step: $STEP" >> "$OUTPUT_DIR/config.txt"
echo "Test started at: $(date)" >> "$OUTPUT_DIR/config.txt"

echo "tps,submitted,confirmed,failed,success_rate" > "$OUTPUT_DIR/results.csv"

for ((tps = START_TPS; tps <= MAX_TPS; tps += STEP)); do
  echo "Testing with TPS target: $tps"
  OUTPUT_FILE="$OUTPUT_DIR/tps_${tps}.json"
  
  # Run the benchmark
  ./target/release/selendra-bench --node-url "$NODE_URL" --backup-url "$BACKUP_URL" \
    --accounts "$ACCOUNTS" --tx-type transfer --tps-target "$tps" --duration "$DURATION" \
    --real-transactions --seed-phrase "$SEED_PHRASE" \
    --min-amount "$MIN_AMOUNT" --max-amount "$MAX_AMOUNT" --output "$OUTPUT_FILE"
  
  # Extract results
  if [ -f "$OUTPUT_FILE" ]; then
    SUBMITTED=$(grep -o '"submitted":[0-9]*' "$OUTPUT_FILE" | cut -d: -f2)
    CONFIRMED=$(grep -o '"status":"confirmed"' "$OUTPUT_FILE" | wc -l)
    FAILED=$(($(cat "$OUTPUT_FILE" | grep -o '"status"' | wc -l) - CONFIRMED))
    
    # Calculate success rate
    if [ "$SUBMITTED" -eq 0 ]; then
      SUCCESS_RATE=0
    else
      SUCCESS_RATE=$(echo "scale=2; 100 * $CONFIRMED / $SUBMITTED" | bc)
    fi
    
    echo "$tps,$SUBMITTED,$CONFIRMED,$FAILED,$SUCCESS_RATE" >> "$OUTPUT_DIR/results.csv"
    
    echo "Results for TPS $tps:" >> "$OUTPUT_DIR/config.txt"
    echo "  Submitted: $SUBMITTED" >> "$OUTPUT_DIR/config.txt"
    echo "  Confirmed: $CONFIRMED" >> "$OUTPUT_DIR/config.txt"
    echo "  Failed: $FAILED" >> "$OUTPUT_DIR/config.txt"
    echo "  Success rate: $SUCCESS_RATE%" >> "$OUTPUT_DIR/config.txt"
    echo "" >> "$OUTPUT_DIR/config.txt"
    
    # Stop testing if success rate drops below 50%
    if (( $(echo "$SUCCESS_RATE < 50" | bc -l) )); then
      echo "Success rate dropped below 50% at TPS $tps"
      echo "This may indicate we've reached maximum network capacity"
      echo "Success rate dropped below 50% at TPS $tps - possible maximum capacity" >> "$OUTPUT_DIR/config.txt"
      break
    fi
  else
    echo "$tps,0,0,0,0" >> "$OUTPUT_DIR/results.csv"
    echo "Error: Output file not created for TPS $tps" >> "$OUTPUT_DIR/config.txt"
  fi
  
  # Wait between tests
  echo "Waiting 30 seconds before the next test..."
  sleep 30
done

echo "TPS testing completed"
echo "Results are in $OUTPUT_DIR/"
echo "Test completed at: $(date)" >> "$OUTPUT_DIR/config.txt"

# Print a summary
echo "TPS,Success Rate" > "$OUTPUT_DIR/summary.txt"
cat "$OUTPUT_DIR/results.csv" | tail -n +2 | cut -d, -f1,5 >> "$OUTPUT_DIR/summary.txt"
cat "$OUTPUT_DIR/summary.txt" 