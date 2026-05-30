#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ "$#" -gt 0 ]; then
  PORTS=("$@")
else
  PORTS=(8000 8001 8002)
fi

export BOOTSTRAP_PEER_IP="${BOOTSTRAP_PEER_IP:-127.0.0.1}"
export BOOTSTRAP_PEER_PORT="${BOOTSTRAP_PEER_PORT:-8000}"
export CHALLENGE_DIFFICULTY="${CHALLENGE_DIFFICULTY:-5}"
export PEER_SYNC_MS="${PEER_SYNC_MS:-1000}"
export AUTOMATION_ENABLED="${AUTOMATION_ENABLED:-true}"
export AUTOMATION_INTERVAL_MS="${AUTOMATION_INTERVAL_MS:-3000}"
export AUCTION_DURATION_MS="${AUCTION_DURATION_MS:-15000}"
export CONSENSUS_MODE="${CONSENSUS_MODE:-por}"

echo "Starting nodes: ${PORTS[*]}"
echo "Press Ctrl-C to stop the network."

pids=()
cleanup() {
  for pid in "${pids[@]}"; do
    kill "$pid" 2>/dev/null || true
  done
}
trap cleanup EXIT INT TERM

for port in "${PORTS[@]}"; do
  (
    cd "$ROOT_DIR"
    cargo run -- --port "$port"
  ) &
  pids+=("$!")
  sleep 1
done

wait
