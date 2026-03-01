#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <profile-log-file>" >&2
  exit 1
fi

raw_log="$1"
if [[ ! -f "$raw_log" ]]; then
  echo "Profile log file not found: $raw_log" >&2
  exit 1
fi

tmp_json="$(mktemp)"
trap 'rm -f "$tmp_json"' EXIT

grep '^GESTALT_PROFILE_JSON:' "$raw_log" | sed 's/^GESTALT_PROFILE_JSON://' > "$tmp_json" || true

if [[ ! -s "$tmp_json" ]]; then
  echo "No GESTALT_PROFILE_JSON records found in: $raw_log" >&2
  echo "Tip: run profile_terminal with --json so the machine-readable summary is emitted." >&2
  exit 1
fi

summary_for_metric() {
  local metric="$1"
  awk -v key="\"${metric}\":" '
    {
      if (match($0, key "[0-9]+")) {
        value = substr($0, RSTART + length(key), RLENGTH - length(key));
        print value;
      }
    }
  ' "$tmp_json" | awk '
    { values[++n] = $1 + 0 }
    END {
      if (n == 0) {
        print "0 - - - -"
        exit
      }

      for (i = 1; i <= n; i++) {
        for (j = i + 1; j <= n; j++) {
          if (values[j] < values[i]) {
            t = values[i]
            values[i] = values[j]
            values[j] = t
          }
        }
      }

      min = values[1]
      max = values[n]
      if (n % 2 == 1) {
        median = values[(n + 1) / 2]
      } else {
        median = (values[n / 2] + values[n / 2 + 1]) / 2
      }
      p95_idx = int(((n - 1) * 95) / 100) + 1
      p95 = values[p95_idx]

      printf "%d %.1f %d %d %d\n", n, median, p95, min, max
    }
  '
}

printf "%-34s %6s %10s %8s %8s %8s\n" "metric" "runs" "median" "p95" "min" "max"
printf "%-34s %6s %10s %8s %8s %8s\n" "----------------------------------" "------" "----------" "--------" "--------" "--------"

for metric in \
  render_pass_p95_us \
  autosave_pass_p95_us \
  baseline_total_send_p95_us \
  render_total_send_p95_us \
  full_total_send_p95_us
  do
    read -r runs median p95 min max <<< "$(summary_for_metric "$metric")"
    printf "%-34s %6s %10s %8s %8s %8s\n" "$metric" "$runs" "$median" "$p95" "$min" "$max"
  done
