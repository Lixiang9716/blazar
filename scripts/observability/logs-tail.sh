#!/usr/bin/env bash
set -euo pipefail

LOG_FILE="${1:-logs/blazar.log}"

test -f "$LOG_FILE" || {
  echo "log file not found: $LOG_FILE" >&2
  exit 2
}

tail -f "$LOG_FILE"
