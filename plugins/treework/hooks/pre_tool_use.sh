#!/usr/bin/env bash
set -euo pipefail

# Guardrail only. Do not mutate project state here.
input="$(cat || true)"

json_escape() {
  local s
  s="$(cat)"
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  s="${s//$'\n'/\\n}"
  s="${s//$'\r'/\\r}"
  s="${s//$'\t'/\\t}"
  printf '"%s"' "$s"
}

if [[ -d ".TreeWork" ]]; then
  case "$input" in
    *".TreeWork/state/"*|*".TreeWork/events.jsonl"*|*"treework:status:start"*|*"treework:root-status:start"*|*"treework:branch-table:start"*)
      msg="TreeWork guardrail: do not manually edit generated state, events, or generated blocks. Use tw transactions."
      escaped="$(printf '%s' "$msg" | json_escape)"
      printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":%s}}\n' "$escaped"
      ;;
  esac
fi
