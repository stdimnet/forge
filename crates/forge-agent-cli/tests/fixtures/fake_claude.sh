#!/bin/sh
# Stand-in for `claude -p --input-format stream-json --output-format stream-json`:
# answers every stdin line with an init (first time), the received args, and a result.
# A message containing "slow" blocks so the caller can interrupt it.
args="$*"
started=""
while IFS= read -r line; do
  if [ -z "$started" ]; then
    echo '{"type":"system","subtype":"init","session_id":"fake-session","model":"fake-model","tools":["Read"]}'
    started=1
  fi
  case "$line" in
    *slow*) sleep 30 ;;
  esac
  printf '{"type":"assistant","message":{"id":"m","content":[{"type":"text","text":"args: %s"}]}}\n' "$args"
  echo '{"type":"result","subtype":"success","is_error":false,"duration_ms":5,"num_turns":1,"result":"ok","total_cost_usd":0.001}'
done
