#!/bin/sh
# Stand-in for `gemini --output-format stream-json`: one process per turn. Reads the whole
# prompt from stdin and answers with stream-json events, reporting the received args, the
# first prompt line and the number of prompt lines.
# A prompt containing "slow" blocks so the caller can interrupt it, "fail" ends with an
# error result (exit code 53, like the CLI's turn limit), "crash" exits without a result and
# "edit" makes a `replace` tool call.
args="$*"
prompt=$(cat)
first=$(printf '%s\n' "$prompt" | head -n 1)
lines=$(printf '%s\n' "$prompt" | wc -l | tr -d ' ')
echo "Loaded cached credentials."
echo '{"type":"init","timestamp":"2026-10-20T09:00:00.000Z","session_id":"fake-gemini-session","model":"gemini-2.5-flash"}'
printf '{"type":"message","role":"user","content":"%s"}\n' "$first"
case "$prompt" in
  *slow*) sleep 30 ;;
  *crash*)
    echo "Please set an Auth method in your settings.json" >&2
    exit 41
    ;;
  *fail*)
    echo '{"type":"result","status":"error","error":{"type":"FatalTurnLimitedError","message":"Reached max session turns"},"stats":{"total_tokens":3,"input_tokens":2,"output_tokens":1,"duration_ms":7,"tool_calls":0}}'
    echo "Reached max session turns" >&2
    exit 53
    ;;
  *edit*)
    printf '{"type":"tool_use","tool_name":"replace","tool_id":"replace-1","parameters":{"file_path":"%s/app.php","old_string":"a","new_string":"b"}}\n' "$PWD"
    echo '{"type":"tool_result","tool_id":"replace-1","status":"success","output":"Successfully modified file"}'
    ;;
esac
printf '{"type":"message","role":"assistant","content":"args: %s","delta":true}\n' "$args"
printf '{"type":"message","role":"assistant","content":" | prompt: %s (%s lines)","delta":true}\n' "$first" "$lines"
echo '{"type":"result","status":"success","stats":{"total_tokens":30,"input_tokens":20,"output_tokens":10,"duration_ms":5,"tool_calls":0}}'
