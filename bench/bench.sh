#!/usr/bin/env bash
set -euo pipefail
root="${1:-${HOME:-.}}"
output="${2:-bench/results.md}"
mkdir -p "$(dirname "$output")"
{
  printf '# Local scan comparison\n\nThis is a local measurement, not a general performance claim.\n\n'
  printf '| Tool | Wall time (seconds) | Exit |\n|---|---:|---:|\n'
  for tool in 'wg scan' 'dust' 'dua'; do
    bin="${tool%% *}"
    if ! command -v "$bin" >/dev/null 2>&1; then continue; fi
    start="$(date +%s)"
    rc=0
    if [[ "$tool" == 'wg scan' ]]; then wg scan "$root" --quiet >/dev/null 2>&1 || rc=$?;
    else "$bin" "$root" >/dev/null 2>&1 || rc=$?; fi
    end="$(date +%s)"
    printf '| `%s` | %s | %s |\n' "$tool" "$((end-start))" "$rc"
  done
} > "$output"
echo "Wrote $output"
