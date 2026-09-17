#!/usr/bin/env bash
# E2E UI test for HKD TT152: boots the REAL app with FRESH data, waits for the
# WebKit inspector, runs e2e.py (DOM automation), then tears the app down.
#
# Usage:
#   ./e2e/run_e2e.sh                    # run every step
#   ./e2e/run_e2e.sh login product      # run only the named steps
#
# Requires a debug binary (src-tauri/target/debug/hkd-tt152-app). If no Vite dev
# server is listening on port 1420 the script starts one and stops it again when
# done (set E2E_KEEP_VITE=1 to leave it running).
#
# Defaults to running on its own Xvfb display so the webview is ALWAYS
# "visible": on Wayland/XWayland, when the desktop session is locked or
# occluded WebKit reports document.visibilityState="hidden" → requestAnimationFrame
# stops → PrimeVue transitions (Dialog/Toast) never finish and elements get
# stuck, making the test flaky. Set E2E_USE_XVFB=0 to reuse the current DISPLAY.
#
# Env overrides:
#   E2E_DATA_HOME   XDG_DATA_HOME for the run (default: <tmp>/hkd-tt152-e2e/data)
#   E2E_LOG_DIR     directory for app/vite/xvfb logs (default: <tmp>/hkd-tt152-e2e/log)
#   E2E_KEEP_VITE   set to 1 to keep the Vite dev server running after the run
#   E2E_INSPECT_PORT, E2E_XVFB_DISPLAY, E2E_USE_XVFB
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
APP="$ROOT/src-tauri/target/debug/hkd-tt152-app"
TMPBASE="${TMPDIR:-/tmp}/hkd-tt152-e2e"
DATA="${E2E_DATA_HOME:-$TMPBASE/data}"
PORT="${E2E_INSPECT_PORT:-9223}"
LOG="${E2E_LOG_DIR:-$TMPBASE/log}"
XVFB_DISPLAY="${E2E_XVFB_DISPLAY:-:99}"
USE_XVFB="${E2E_USE_XVFB:-1}"
KEEP_VITE="${E2E_KEEP_VITE:-0}"
APP_PID=""
XVFB_PID=""
VITE_PID=""
mkdir -p "$LOG"

cleanup() {
  kill "$APP_PID" 2>/dev/null
  wait "$APP_PID" 2>/dev/null
  if [ -n "$XVFB_PID" ]; then
    kill "$XVFB_PID" 2>/dev/null
    wait "$XVFB_PID" 2>/dev/null
  fi
  # Stop the Vite server we started (never kill one that was already running).
  if [ "$KEEP_VITE" != "1" ] && [ -n "$VITE_PID" ]; then
    pkill -P "$VITE_PID" 2>/dev/null
    kill "$VITE_PID" 2>/dev/null
    wait "$VITE_PID" 2>/dev/null
  fi
}

if [ ! -x "$APP" ]; then
  echo "[runner] ERROR: missing binary $APP — run: (cd src-tauri && cargo build)"
  exit 2
fi

# 0) Vite dev server (the debug app loads the frontend from http://localhost:1420)
if ! ss -ltn 2>/dev/null | grep -q ':1420'; then
  echo "[runner] Vite not running — starting..."
  ( cd "$ROOT" && exec nohup bun run dev > "$LOG/vite.log" 2>&1 ) &
  VITE_PID=$!
fi
# Wait until Vite actually serves (cold start still optimizes deps)
for _ in $(seq 1 120); do
  curl -sf -o /dev/null "http://localhost:1420/" 2>/dev/null && break
  sleep 0.5
done

# 0b) Dedicated Xvfb display (webview stays visible → rAF runs → transitions finish)
if [ "$USE_XVFB" = "1" ]; then
  if DISPLAY="$XVFB_DISPLAY" xdpyinfo >/dev/null 2>&1; then
    echo "[runner] reusing existing Xvfb $XVFB_DISPLAY"
  else
    echo "[runner] starting Xvfb $XVFB_DISPLAY..."
    Xvfb "$XVFB_DISPLAY" -screen 0 1400x900x24 > "$LOG/xvfb.log" 2>&1 &
    XVFB_PID=$!
    for _ in $(seq 1 40); do
      DISPLAY="$XVFB_DISPLAY" xdpyinfo >/dev/null 2>&1 && break
      sleep 0.3
    done
  fi
  export DISPLAY="$XVFB_DISPLAY"
fi
export GDK_BACKEND=x11

# 1) Kill any old app instance and wipe previous data
pkill -x hkd-tt152-app 2>/dev/null
sleep 0.5
rm -rf "$DATA"

# 2) Start the app in the background (X11 + inspector)
echo "[runner] starting app (fresh data: $DATA, DISPLAY=$DISPLAY)..."
XDG_DATA_HOME="$DATA" GDK_BACKEND=x11 DISPLAY="$DISPLAY" \
  WEBKIT_INSPECTOR_HTTP_SERVER="127.0.0.1:$PORT" \
  "$APP" > "$LOG/app.log" 2>&1 &
APP_PID=$!

# 3) Wait for the inspector port
for _ in $(seq 1 60); do
  ss -ltn 2>/dev/null | grep -q ":$PORT" && break
  sleep 0.5
done
if ! ss -ltn 2>/dev/null | grep -q ":$PORT"; then
  echo "[runner] ERROR: inspector did not listen on port $PORT"; tail -20 "$LOG/app.log"; cleanup; exit 2
fi
sleep 3.0  # give the webview time to render the frontend (e2e.py waits more anyway)

# 3b) Excel fixture for the `import` step (generated with SheetJS if missing)
FIXTURE="$HERE/fixtures/nhap-lieu-e2e.xlsx"
if [ ! -f "$FIXTURE" ]; then
  (cd "$ROOT" && bun "$HERE/make_fixtures.mjs" "$FIXTURE") \
    || echo "[runner] warning: could not generate the import fixture"
fi

# 4) Run the tests (E2E_DB points at the runtime DB so checks stay independent of the UI)
E2E_DB="$DATA/git.shin.hdk-tt152-app/profiles/default/hkd.db" \
  E2E_IMPORT_FILE="$FIXTURE" \
  python3 "$HERE/e2e.py" "$@"
RC=$?

# 5) Clean up
cleanup
echo "[runner] done (rc=$RC)"
exit $RC
