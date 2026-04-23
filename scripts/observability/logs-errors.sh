#!/usr/bin/env bash
set -euo pipefail

LOG_FILE="${1:-logs/blazar.log}"

test -f "$LOG_FILE" || {
  echo "log file not found: $LOG_FILE" >&2
  exit 2
}

command -v jq >/dev/null 2>&1 || {
  python3 - "$LOG_FILE" <<'PY'
import json
import sys

path = sys.argv[1]
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
        if record.get("level") in ("WARN", "ERROR"):
            print(json.dumps(record, separators=(",", ":")))
PY
  exit 0
}

jq -c 'select(.level == "WARN" or .level == "ERROR")' "$LOG_FILE"
