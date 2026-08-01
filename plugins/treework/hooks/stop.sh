#!/usr/bin/env bash
set -euo pipefail

TW="$PLUGIN_ROOT/skills/treework/scripts/tw"

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
  check="$("$TW" check --brief 2>&1 || true)"
  if ! printf '%s' "$check" | grep -q "0 issue(s)"; then
    msg="TreeWork stop check needs attention. $check"
    escaped="$(printf '%s' "$msg" | json_escape)"
    printf '{"continue":true,"systemMessage":%s}\n' "$escaped"
  fi
fi
