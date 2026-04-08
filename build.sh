#!/usr/bin/env bash
set -euo pipefail

cargo build --release

echo "Binary: target/release/tpn-worker-dashboard"
ls -lh target/release/tpn-worker-dashboard

cp -f -v target/release/tpn-worker-dashboard ./
