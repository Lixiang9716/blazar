#!/usr/bin/env bash
set -euo pipefail

TOOLS=(jq lnav fzf)
missing=()

usage() {
  cat <<'USAGE'
Usage: install-tools.sh [--check|--install]

Environment:
  CHECK_ONLY=1   Report missing tools without installing
USAGE
}

is_truthy() {
  case "${1:-}" in
    1 | true | TRUE | yes | YES | on | ON) return 0 ;;
    *) return 1 ;;
  esac
}

detect_package_manager() {
  if command -v apt-get >/dev/null 2>&1; then
    echo "apt"
    return
  fi
  if command -v brew >/dev/null 2>&1; then
    echo "brew"
    return
  fi
  echo "none"
}

check_tools() {
  local tool
  missing=()
  for tool in "${TOOLS[@]}"; do
    if command -v "$tool" >/dev/null 2>&1; then
      echo "✅ $tool already installed"
    else
      echo "⚠️  $tool not found"
      missing+=("$tool")
    fi
  done
}

check_only=false
case "${1:-}" in
  --check)
    check_only=true
    shift
    ;;
  --install)
    check_only=false
    shift
    ;;
  "")
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac

if [ "$#" -gt 0 ]; then
  usage >&2
  exit 2
fi

if is_truthy "${CHECK_ONLY:-0}"; then
  check_only=true
fi

package_manager="$(detect_package_manager)"
if [ "$package_manager" = "none" ]; then
  echo "Package manager: none detected"
else
  echo "Package manager: $package_manager"
fi

check_tools

if ((${#missing[@]} == 0)); then
  echo "All observability tools are available."
  exit 0
fi

echo "Missing tools: ${missing[*]}"

if $check_only; then
  echo "Check-only mode: no installation attempted."
  case "$package_manager" in
    apt) echo "Action: run 'sudo apt-get install -y ${missing[*]}'" ;;
    brew) echo "Action: run 'brew install ${missing[*]}'" ;;
    *) echo "Action: install manually for your platform: ${missing[*]}" ;;
  esac
  exit 1
fi

echo "Install mode: attempting automatic installation."
case "$package_manager" in
  apt)
    if command -v sudo >/dev/null 2>&1; then
      install_cmd=(sudo apt-get install -y "${missing[@]}")
    else
      install_cmd=(apt-get install -y "${missing[@]}")
    fi
    ;;
  brew)
    install_cmd=(brew install "${missing[@]}")
    ;;
  *)
    echo "No supported package manager detected for automatic install."
    echo "Action: install manually, then re-run this script."
    exit 1
    ;;
esac

echo "Running: ${install_cmd[*]}"
if ! "${install_cmd[@]}"; then
  echo "Automatic installation failed."
  case "$package_manager" in
    apt) echo "Action: retry manually with 'sudo apt-get install -y ${missing[*]}'" ;;
    brew) echo "Action: retry manually with 'brew install ${missing[*]}'" ;;
    *) echo "Action: install manually for your platform: ${missing[*]}" ;;
  esac
  exit 1
fi

echo "Re-checking installed tools..."
check_tools
if ((${#missing[@]} == 0)); then
  echo "All observability tools are available."
  exit 0
fi

echo "Some tools are still missing: ${missing[*]}"
echo "Action: install missing tools manually and rerun."
exit 1
