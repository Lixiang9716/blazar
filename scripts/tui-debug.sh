#!/bin/bash
# Blazar TUI Screenshot & Compare Tool
# Usage:
#   ./scripts/tui-debug.sh              # Take screenshot of current TUI
#   ./scripts/tui-debug.sh compare      # Take TUI + mockup comparison
#   ./scripts/tui-debug.sh start        # Start ttyd server
#   ./scripts/tui-debug.sh stop         # Stop ttyd server
#   ./scripts/tui-debug.sh status       # Check ttyd status

BLAZAR_DIR="/home/lx/blazar"
TTYD_PORT=8090
OUT_DIR="/tmp/tui-screenshots"
MOCKUP_HTML="/tmp/brainstorm-2294366-1776600218/content/slime-claude-interactions.html"
COMPARE_SCRIPT="/tmp/tui-compare.js"
SCREENSHOT_SCRIPT="/tmp/tui-screenshot.js"

case "${1:-screenshot}" in
  start)
    if ss -tlnp 2>/dev/null | grep -q ":${TTYD_PORT}"; then
      echo "ttyd already running on port $TTYD_PORT"
    else
      echo "Building blazar..."
      cd "$BLAZAR_DIR" && cargo build --release 2>&1 | tail -3
      echo "Starting ttyd on port $TTYD_PORT..."
      ttyd -p "$TTYD_PORT" "$BLAZAR_DIR/target/release/blazar" &
      sleep 2
      echo "ttyd started (PID: $!)"
    fi
    ;;

  stop)
    PID=$(ss -tlnp 2>/dev/null | grep ":${TTYD_PORT}" | grep -oP 'pid=\K[0-9]+')
    if [ -n "$PID" ]; then
      kill "$PID" 2>/dev/null
      echo "Stopped ttyd (PID: $PID)"
    else
      echo "ttyd not running on port $TTYD_PORT"
    fi
    ;;

  status)
    if ss -tlnp 2>/dev/null | grep -q ":${TTYD_PORT}"; then
      echo "✅ ttyd running on http://localhost:${TTYD_PORT}"
    else
      echo "❌ ttyd not running. Use: $0 start"
    fi
    ;;

  screenshot)
    mkdir -p "$OUT_DIR"
    echo "Taking TUI screenshot..."
    cd /tmp && node "$SCREENSHOT_SCRIPT" 2>&1
    LATEST=$(ls -t "$OUT_DIR"/blazar-tui-2*.png 2>/dev/null | head -1)
    [ -n "$LATEST" ] && echo "Latest: $LATEST"
    ;;

  compare)
    mkdir -p "$OUT_DIR"
    NAME="${2:-compare}"
    WAIT="${3:-4000}"
    echo "Taking TUI + Mockup comparison screenshots..."
    cd /tmp && node "$COMPARE_SCRIPT" \
      --name "$NAME" \
      --wait "$WAIT" \
      --mockup "file://${MOCKUP_HTML}?play=1" \
      2>&1
    echo ""
    echo "Open: $OUT_DIR/latest-comparison.html"
    ;;

  *)
    echo "Usage: $0 {start|stop|status|screenshot|compare [name] [wait_ms]}"
    ;;
esac
