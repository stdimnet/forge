#!/bin/sh
# Stand-in for `claude -p --input-format stream-json --output-format stream-json`:
# answers every stdin line with an init (first time), the received args, and a result.
# A message containing "slow" blocks so the caller can interrupt it.
# A message containing "needs-permission" asks to run `ls` with a `can_use_tool` control
# request, waits for the control response and reports the behavior it received.
args="$*"
started=""
n=0
while IFS= read -r line; do
  if [ -z "$started" ]; then
    echo '{"type":"system","subtype":"init","session_id":"fake-session","model":"fake-model","tools":["Read"]}'
    started=1
  fi
  case "$line" in
    *slow*) sleep 30 ;;
    *needs-permission*)
      n=$((n + 1))
      printf '{"type":"control_request","request_id":"perm-%s","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"ls"},"tool_use_id":"toolu_%s"}}\n' "$n" "$n"
      IFS= read -r answer
      case "$answer" in
        *'"request_id":"perm-'"$n"'"'*'"behavior":"allow"'*'"updatedInput":{"command":"ls"}'*) decision="allow" ;;
        *'"request_id":"perm-'"$n"'"'*'"behavior":"deny"'*) decision="deny" ;;
        *) decision="invalid" ;;
      esac
      printf '{"type":"assistant","message":{"id":"p%s","content":[{"type":"text","text":"permission: %s"}]}}\n' "$n" "$decision"
      ;;
  esac
  printf '{"type":"assistant","message":{"id":"m","content":[{"type":"text","text":"args: %s"}]}}\n' "$args"
  echo '{"type":"result","subtype":"success","is_error":false,"duration_ms":5,"num_turns":1,"result":"ok","total_cost_usd":0.001}'
done
