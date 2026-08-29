#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
API_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

echo "==> Running Ledger Reconciliation Audit..."
cd "${API_DIR}"
cargo run --bin reconcile
