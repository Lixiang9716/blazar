#!/usr/bin/env bash
set -euo pipefail

TURN_ID="${1:-}"
TURN_ID="$(printf '%s' "$TURN_ID" | sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//')"
LOG_FILE="${2:-logs/blazar.log}"

test -n "$TURN_ID" || {
  echo "usage: logs-turn.sh <turn_id> [log_file]" >&2
  exit 2
}

test -f "$LOG_FILE" || {
  echo "log file not found: $LOG_FILE" >&2
  exit 2
}

command -v jq >/dev/null 2>&1 || {
  python3 - "$TURN_ID" "$LOG_FILE" <<'PY'
import json
import sys

turn_id = sys.argv[1]
path = sys.argv[2]
with open(path, "r", encoding="utf-8") as handle:
    for line_no, raw in enumerate(handle, start=1):
        line = raw.strip()
        if not line:
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError as error:
            print(f"invalid json in {path}:{line_no}: {error}", file=sys.stderr)
            sys.exit(1)
        if record.get("turn_id") == turn_id:
            print(json.dumps(record, separators=(",", ":")))
PY
  exit 0
}

jq -c --arg turn "$TURN_ID" 'select(.turn_id == $turn)' "$LOG_FILE"
