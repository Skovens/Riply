#!/usr/bin/env bash
# e2e-movie.sh — Riply end-to-end test on real hardware (movie disc).
#
# You MUST run this in your own terminal (it needs a /dev/sr* drive with the
# disc and makemkvcon); sandboxed/containers without block devices cannot
# exercise a real disc.
#
#   ./scripts/e2e-movie.sh
#
# It drives the server only over its HTTP API (port from config.toml) and
# asserts: server start, drive discovery, disc-name resolution, TMDb
# auto-naming, manual search, rip with live SSE progress, accept → move into
# movies_dir ({Title} ({Year})/{Title}.mkv), library entry, metrics, eject,
# duplicate-disc warning on re-insert, config PUT with "~" expansion, and a
# hardware-health check of the kernel log for USB drops.
#
# Env overrides: RIP_TIMEOUT=3600 (max rip minutes), SKIP_DUPLICATE=1,
# KEEP_SERVER=1 (leave riply running afterwards).

set -uo pipefail

cd "$(dirname "$0")/.."
REPO="$PWD"
START_TS=$(date +%s)

# ---- config ---------------------------------------------------------------
BIN="$REPO/riply"
LOG="/tmp/riply-e2e-server.log"
MKV_LOG="/tmp/riply-e2e-makemkv.log"
SSE_LOG="/tmp/riply-e2e-sse.log"
CFG="${XDG_CONFIG_HOME:-$HOME/.config}/riply/config.toml"
PORT=$(grep -oP '^\s*http_port\s*=\s*\K\d+' "$CFG" 2>/dev/null || echo 3113)
BASE="http://localhost:$PORT"
RIP_TIMEOUT=${RIP_TIMEOUT:-3600}
SKIP_DUPLICATE=${SKIP_DUPLICATE:-0}
KEEP_SERVER=${KEEP_SERVER:-0}

SERVER_PID=""
SSE_PID=""
FAILED=0
declare -a RESULTS=()

# ---- helpers --------------------------------------------------------------
say()  { printf '\n\033[1;36m▶ %s\033[0m\n' "$*"; }
pass() { RESULTS+=("PASS  $*"); printf '  \033[1;32mPASS\033[0m %s\n' "$*"; }
fail() { RESULTS+=("FAIL  $*"); printf '  \033[1;31mFAIL\033[0m %s\n' "$*"; FAILED=$((FAILED+1)); }
check(){ local name="$1"; shift; if "$@" >/dev/null 2>&1; then pass "$name"; else fail "$name"; fi; }

# wait_for <timeout_secs> <description> <cmd...>
wait_for() {
  local t="$1" desc="$2"; shift 2
  local end=$(( $(date +%s) + t ))
  while (( $(date +%s) < end )); do
    if "$@" >/dev/null 2>&1; then return 0; fi
    sleep 2
  done
  echo "    timed out after ${t}s: $desc" >&2
  return 1
}

api()  { curl -sS -m 15 "$@"; }          # curl with sane defaults
jqr()  { jq -r "$@" 2>/dev/null; }       # jq read, null-safe (jq options first, filter last)

saw_event() { grep -q "\"type\":\"$1\"" "$SSE_LOG" 2>/dev/null; }
event_json() { grep "\"type\":\"$1\"" "$SSE_LOG" | tail -1 | sed 's/^data: //'; }

# wait_for helper predicates — read the globals, so no argument-quoting
# gymnastics through sh -c (wait_for runs its command with no extra args).
any_drive_ready() { api "$BASE/api/drives" | jq -e 'map(.state=="ready") | any' >/dev/null; }
drive_has_disc()  { api "$BASE/api/drives" | jq -e --argjson i "$DISC_INDEX" '.[$i] | (.state=="ready") and (.disc_name|length>0)' >/dev/null; }
drive_named()     { api "$BASE/api/drives" | jq -e --argjson i "$DISC_INDEX" '.[$i].canonical_name? // "" | length > 0' >/dev/null; }
drive_state()     { api "$BASE/api/drives" | jq -e --argjson i "$DISC_INDEX" --arg s "$1" '.[$i].state == $s' >/dev/null; }

diag_hardware() {
  echo "  ---- hardware hints ----"
  echo "  USB disconnects in kernel log since test start:"
  journalctl -k --no-pager --since "@$START_TS" 2>/dev/null | grep -cE 'USB disconnect' | sed 's/^/    /'
  journalctl -k --no-pager --since "@$START_TS" 2>/dev/null | grep -E 'USB disconnect|Maybe the USB cable|DID_ERROR|No such device|scsi .* CD-ROM' | tail -8 | sed 's/^/    /'
  echo "  MakeMKV raw capture: $MKV_LOG"
}

cleanup() {
  [[ -n "$SSE_PID"  ]] && kill "$SSE_PID"  2>/dev/null
  [[ -n "$SERVER_PID" && "$KEEP_SERVER" != 1 ]] && kill "$SERVER_PID" 2>/dev/null
  wait 2>/dev/null
}
trap cleanup EXIT

# ---- 0. preflight ---------------------------------------------------------
say "Preflight"
for tool in makemkvcon curl jq eject lsblk; do
  check "found $tool" command -v "$tool"
done

if ls /dev/sr* >/dev/null 2>&1; then pass "optical drive present ($(ls /dev/sr* | tr '\n' ' '))"; else
  fail "no /dev/sr* — is a drive powered and plugged in? (kernel sees: $(ls /sys/block 2>/dev/null | grep -c '^sr' | tr -d '\n') device(s)); if sysfs has the node but /dev is missing, the udev node was not created (replug; as root: 'udevadm trigger --action=add --sysname-match=sr*')"
  exit 1
fi

# Kill any riply left from a previous session (holds the port / stale state).
pkill -f "$BIN" 2>/dev/null; sleep 1
if curl -s -o /dev/null -m 2 "$BASE/api/metrics"; then
  fail "port $PORT still served by something else"; exit 1
fi
check "build binary" go build -o "$BIN" ./cmd/server/

# ---- 1. start server ------------------------------------------------------
say "Start server"
: > "$SSE_LOG"
RIPLY_MAKEMKV_LOG="$MKV_LOG" "$BIN" >"$LOG" 2>&1 &
SERVER_PID=$!
if wait_for 30 "server answering $BASE/api/metrics" curl -sf -o /dev/null -m 3 "$BASE/api/metrics"; then
  pass "server up (pid $SERVER_PID)"
else fail "server did not come up (see $LOG)"; tail -20 "$LOG"; exit 1
fi

check "GET / serves web UI"            sh -c "curl -sS -m 5 '$BASE/' | grep -qi '<html'"
check "GET /api/config"                sh -c "curl -sS -m 5 '$BASE/api/config' | jq -e '.movies_dir' >/dev/null"
check "GET /api/library"               sh -c "curl -sS -m 5 '$BASE/api/library' | jq -e '.entries' >/dev/null"
check "GET /api/metrics"               sh -c "curl -sS -m 5 '$BASE/api/metrics' | jq -e '.total_rips != null' >/dev/null"

# SSE subscriber must be attached before anything happens.
curl -sS -N "$BASE/events" >"$SSE_LOG" &
SSE_PID=$!
sleep 1
kill -0 "$SSE_PID" && pass "SSE stream attached" || fail "SSE stream could not attach"

# ---- 2. config: "~" expansion (runtime PUT) --------------------------------
say "Config: PUT movies_dir=\"~/Movies\" expands to the real home path"
MOVIES="$HOME/Movies"
# /tmp is often a small tmpfs (a DVD rip is several GB); let the temp dir
# live on a bigger filesystem. Override with E2E_TEMP_DIR.
TEMP_DIR="${E2E_TEMP_DIR:-/var/tmp/riply-e2e-temp}"
code=$(curl -sS -m 10 -o /tmp/riply-e2e-cfg.json -w '%{http_code}' -X PUT "$BASE/api/config" \
  -d "{\"movies_dir\":\"~/Movies\",\"temp_dir\":\"$TEMP_DIR\"}")
if [[ "$code" == 200 ]]; then pass "PUT /api/config returned 200"; else
  fail "PUT /api/config returned $code"; cat /tmp/riply-e2e-cfg.json; exit 1
fi
cfg_movies=$(jq -r '.movies_dir' /tmp/riply-e2e-cfg.json)
if [[ "$cfg_movies" == "$MOVIES" ]]; then pass "movies_dir expanded to $MOVIES (no literal ~)"
else fail "movies_dir = '$cfg_movies', expected '$MOVIES'"; fi
check "no literal ./~ directory created" test ! -e "$REPO/~"

# ---- 3. drive discovery ---------------------------------------------------
say "Insert a MOVIE disc now (drive must be empty first)"
read -rt 120 -p "  Press Enter after the disc is fully in... " _ || true

# Discover the drive that actually has the disc. The API returns drives
# sorted by device path; a VM may attach an empty drive (e.g. a QEMU
# DVD-ROM) at a lower index than the real one, so index 0 is not
# guaranteed to be the disc's drive. Pick the first drive whose media probe
# says "ready" (a disc is present and readable).
if ! wait_for 90 "a drive becomes ready (disc present)" any_drive_ready; then
  fail "no drive with a disc found via API"; api "$BASE/api/drives" | jq .; diag_hardware; exit 1
fi
DISC_INDEX=$(api "$BASE/api/drives" | jq -r 'to_entries | map(select(.value.state=="ready"))[0].key')
DRIVE_PATH=$(api "$BASE/api/drives" | jq -r --argjson i "$DISC_INDEX" '.[$i].path')
if ! [[ "$DISC_INDEX" =~ ^[0-9]+$ ]] || [[ -z "$DRIVE_PATH" ]]; then
  fail "could not determine the disc's drive (index=$DISC_INDEX path=$DRIVE_PATH)"; exit 1
fi
pass "disc drive discovered: $DRIVE_PATH (index $DISC_INDEX)"

# ---- 4. disc name resolution + auto-naming --------------------------------
say "Disc-name resolution (GetDiscInfo) + TMDb auto-naming"
if ! wait_for 120 "drive state ready with disc_name" drive_has_disc; then
  fail "disc never became ready"; api "$BASE/api/drives" | jq .; diag_hardware; exit 1
fi
DISC_NAME=$(api "$BASE/api/drives" | jqr --argjson i "$DISC_INDEX" '.[$i].disc_name')
pass "disc resolved: \"$DISC_NAME\""
saw_event drive_ready && pass "SSE drive_ready" || fail "SSE drive_ready missing"

if ! wait_for 30 "drive canonical_name set by auto-naming" drive_named; then
  fail "auto-naming produced no TMDb match"; diag_hardware; exit 1
fi
TITLE=$(api "$BASE/api/drives" | jqr --argjson i "$DISC_INDEX" '.[$i].canonical_name')
YEAR=$(api "$BASE/api/drives" | jqr --argjson i "$DISC_INDEX" '.[$i].canonical_year')
pass "auto-named: \"$TITLE ($YEAR)\""
saw_event naming_match && pass "SSE naming_match" || fail "SSE naming_match missing"

# ---- 5. manual search ------------------------------------------------------
say "Manual TMDb search"
search_count=$(api "$BASE/api/search?q=$(jq -rn --arg t "$TITLE" '$t|@uri')" | jqr 'length')
if [[ "${search_count:-0}" -gt 0 ]]; then pass "GET /api/search returned $search_count results"
else fail "GET /api/search returned nothing"; fi

# ---- 6. rip ----------------------------------------------------------------
say "Start rip (drive accept pre-flow: none — we accept manually after rip)"
START_CODE=$(curl -sS -m 15 -o /tmp/riply-e2e-start.json -w '%{http_code}' -X POST "$BASE/api/drives/$DISC_INDEX/start?path=$DRIVE_PATH")
if [[ "$START_CODE" == 200 ]]; then
  RIP_ID=$(jq -r '.rip_id' /tmp/riply-e2e-start.json)
  pass "rip started (id $RIP_ID)"
else
  fail "POST start returned $START_CODE"; cat /tmp/riply-e2e-start.json; exit 1
fi

echo "  Ripping… (timeout ${RIP_TIMEOUT}s; progress below)"
STATE=""
LAST_PCT=""
end=$(( $(date +%s) + RIP_TIMEOUT ))
while (( $(date +%s) < end )); do
  RIPS=$(api "$BASE/api/rips")
  STATE=$(echo "$RIPS" | jqr ".[] | select(.id==$RIP_ID) | .state")
  PCT=$(echo "$RIPS" | jq -r --argjson id "$RIP_ID" '.[] | select(.id==$id) | .progress?.percent // 0' | tail -1)
  PHASE=$(echo "$RIPS" | jq -r --argjson id "$RIP_ID" '.[] | select(.id==$id) | .progress?.phase // ""' | tail -1)
  PCT=${PCT:-0}
  [[ "$PCT" != "$LAST_PCT" ]] && printf '    %3.0f%%  %s  (%s)\n' "$PCT" "${PHASE:-…}" "$(date +%T)" && LAST_PCT="$PCT"
  case "$STATE" in
    complete) break ;;
    error)    fail "rip failed"; echo "    $(echo "$RIPS" | jqr ".[] | select(.id==$RIP_ID) | .error")"; break ;;
    cancelled) fail "rip was cancelled unexpectedly"; break ;;
  esac
  if saw_event rip_failed; then fail "SSE rip_failed received"; break; fi
  sleep 3
done

if [[ "$STATE" != "complete" ]]; then
  fail "rip did not reach complete (state='$STATE')"
  echo "  last MakeMKV output:"; tail -6 "$MKV_LOG" | sed 's/^/    /'
  diag_hardware; exit 1
fi
pass "rip reached complete"
PROG_COUNT=$(grep -c '"type":"rip_progress"' "$SSE_LOG")
if [[ "$PROG_COUNT" -ge 3 ]]; then pass "SSE rip_progress streamed ($PROG_COUNT events)"
else fail "only $PROG_COUNT rip_progress events"; fi
saw_event rip_complete && pass "SSE rip_complete" || fail "SSE rip_complete missing"

# ---- 7. accept → move to library -------------------------------------------
say "Accept naming → move to library"
ACC_CODE=$(curl -sS -m 300 -o /tmp/riply-e2e-accept.json -w '%{http_code}' -X POST "$BASE/api/rips/$RIP_ID/accept")
if [[ "$ACC_CODE" == 200 ]]; then pass "POST accept returned 200"; else
  fail "POST accept returned $ACC_CODE"; cat /tmp/riply-e2e-accept.json; exit 1
fi
DEST=$(jq -r '.destination'  /tmp/riply-e2e-accept.json)
MAIN=$(jq -r '.main_file'    /tmp/riply-e2e-accept.json)
SRC=$(jq -r  '.source'       /tmp/riply-e2e-accept.json)
echo "    destination: $DEST"
check "main file exists"        test -s "$MAIN"
check "destination under movies_dir" test "${DEST#"$MOVIES"/}" != "$DEST"
check "rip temp dir cleaned up"  test ! -e "$SRC"
saw_event rip_accepted && pass "SSE rip_accepted" || fail "SSE rip_accepted missing"

# ---- 8. library + metrics ---------------------------------------------------
say "Library index + metrics"
# The disc's media type is whatever classifyRip decides (a multi-title rip
# is "tv", a single big file is "movie"), so assert only that the main
# file is indexed as a rip — not which type.
lib_hit=$(api "$BASE/api/library" | jq --arg p "$MAIN" '[.entries[] | select(.output_path==$p and .source=="rip")] | length')
if [[ "${lib_hit:-0}" -ge 1 ]]; then pass "library contains the ripped media ($lib_hit entr$( [ "$lib_hit" == 1 ] && echo y || echo ies))"
else fail "library missing entry for $MAIN"; api "$BASE/api/library" | jq '.entries | length'; fi
m=$(api "$BASE/api/metrics")
ok=$(jq -e '.total_rips >= 1 and .successful_rips >= 1 and .failed_rips == 0 and .active_rips == 0' <<<"$m" >/dev/null && echo 1)
if [[ "$ok" == 1 ]]; then pass "metrics: $(jq -c . <<<"$m")"
else fail "unexpected metrics: $(jq -c . <<<"$m")"; fi

# ---- 9. eject ---------------------------------------------------------------
say "Eject"
EJ_CODE=$(curl -sS -m 60 -o /tmp/riply-e2e-eject.json -w '%{http_code}' -X POST "$BASE/api/drives/$DISC_INDEX/eject?path=$DRIVE_PATH")
if [[ "$EJ_CODE" == 200 ]]; then pass "POST eject returned 200 (tray should open now)"
else fail "POST eject returned $EJ_CODE"; cat /tmp/riply-e2e-eject.json; fi
wait_for 30 "poller notices the disc gone" drive_state no_disc \
  && pass "drive state no_disc after eject" || fail "drive state never became no_disc"
saw_event drive_ejected && pass "SSE drive_ejected" || fail "SSE drive_ejected missing"

# ---- 10. duplicate-disc warning (re-insert) ---------------------------------
if [[ "$SKIP_DUPLICATE" != 1 ]]; then
  say "Re-insert the SAME disc (duplicate-disc warning check)"
  read -rt 120 -p "  Press Enter after the disc is fully in (or Ctrl-C to skip)..." _ || SKIP_DUPLICATE=1
  if [[ "$SKIP_DUPLICATE" != 1 ]]; then
    if wait_for 90 "disc ready again" drive_state ready; then
      if wait_for 30 "rip_duplicate SSE event" sh -c "grep -q '\"type\":\"rip_duplicate\"' '$SSE_LOG'"; then
        pass "duplicate disc warned: $(event_json rip_duplicate | jq -r .message)"
      else fail "no rip_duplicate event for a re-ripped disc"; fi
      read -rt 30 -p "  Eject when done, press Enter… " _ || true
    else fail "disc never became ready again"; fi
  fi
fi

# ---- 11. remaining endpoint smokes ------------------------------------------
say "Endpoint smokes (toggles, name set/undo, library scan)"
check "PUT auto-naming=false"   sh -c "curl -sS -m 5 -X PUT '$BASE/api/drives/$DISC_INDEX/auto-naming' -d '{\"enabled\":false}' | jq -e '.status==\"ok\"' >/dev/null"
check "PUT auto-naming=true"    sh -c "curl -sS -m 5 -X PUT '$BASE/api/drives/$DISC_INDEX/auto-naming' -d '{\"enabled\":true}'  | jq -e '.status==\"ok\"' >/dev/null"
check "PUT auto-sequence=false" sh -c "curl -sS -m 5 -X PUT '$BASE/api/drives/$DISC_INDEX/auto-sequence' -d '{\"enabled\":false}' | jq -e '.status==\"ok\"' >/dev/null"
check "PUT auto-sequence=true"  sh -c "curl -sS -m 5 -X PUT '$BASE/api/drives/$DISC_INDEX/auto-sequence' -d '{\"enabled\":true}' | jq -e '.status==\"ok\"' >/dev/null"
check "PUT /api/name"           sh -c "curl -sS -m 5 -X PUT '$BASE/api/drives/$DISC_INDEX/name' -d '{\"title\":\"Smoke\",\"year\":1994}' | jq -e '.status==\"ok\"' >/dev/null"
check "POST drive undo-accept"  sh -c "curl -sS -m 5 -X POST '$BASE/api/drives/$DISC_INDEX/undo-accept' | jq -e '.status==\"ok\"' >/dev/null"
scan_code=$(curl -sS -m 10 -o /dev/null -w '%{http_code}' -X POST "$BASE/api/library/scan")
[[ "$scan_code" == 202 ]] && pass "POST /api/library/scan accepted" || fail "POST /api/library/scan returned $scan_code"
wait_for 60 "library_scanned SSE" sh -c "grep -q '\"type\":\"library_scanned\"' '$SSE_LOG'" \
  && pass "SSE library_scanned" || fail "SSE library_scanned missing"

# ---- 12. hardware health during the run --------------------------------------
say "Hardware health during test"
FLAPS=$(journalctl -k --no-pager --since "@$START_TS" 2>/dev/null | grep -cE 'USB disconnect')
if [[ "$FLAPS" == "0" ]]; then pass "no USB disconnects during the run"
else fail "$FLAPS USB disconnect(s) during the run — cabling/power is unstable, expect failures:"; diag_hardware; fi

# ---- summary ----------------------------------------------------------------
say "Summary"
printf '  %s\n' "${RESULTS[@]}"
echo
if [[ "$FAILED" == 0 ]]; then echo "  ALL E2E CHECKS PASSED — movie landed at:"; echo "    $MAIN"; exit 0
else echo "  $FAILED CHECK(S) FAILED (see above)"; diag_hardware; exit 1; fi
