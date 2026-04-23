#!/usr/bin/env bash
set -euo pipefail
# Task 3 placeholder: non-installing status checker. Task 4 owns full auto-install flow.

TOOLS=(jq lnav fzf)
missing=()

for tool in "${TOOLS[@]}"; do
  if command -v "$tool" >/dev/null 2>&1; then
    echo "✅ $tool already installed"
  else
    echo "⚠️  $tool not found"
    missing+=("$tool")
  fi
done

if ((${#missing[@]} == 0)); then
  echo "All observability tools are available."
  exit 0
fi

echo "Missing tools: ${missing[*]}"
echo "Task 3 placeholder: automatic install flow is added in Task 4."
echo "Install manually for now (examples):"
echo "  - Ubuntu/Debian: sudo apt-get install -y ${missing[*]}"
echo "  - macOS (Homebrew): brew install ${missing[*]}"
