#!/usr/bin/env bash
set -euo pipefail

# Runs the terminal latency regression gate in environments with PTY support.
# Disable with GESTALT_SKIP_PERF_GATE=1 when PTY access is unavailable.
if [[ "${GESTALT_SKIP_PERF_GATE:-0}" == "1" ]]; then
  echo "Skipping perf gate (GESTALT_SKIP_PERF_GATE=1)."
  exit 0
fi

cargo run --quiet --bin profile_terminal -- --assert "$@"
