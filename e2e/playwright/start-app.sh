#!/usr/bin/env bash
# Boots the app in dev mode (`bun run tauri dev`) for Playwright, fully isolated:
#   - Ports:     HKD_WEB_PORT=45821 (app's embedded web server)
#                VITE_PORT=1421      (Vite dev server — avoids the user's 1420)
#                VITE_API_PROXY → http://127.0.0.1:45821 (Vite /api proxy target)
#   - Data:      HKD_DATA_DIR=<temp dir> — its own DB, never touches real data.
# The most recent DB path is stored in /tmp/hkd-e2e-dir for inspection.
set -u

# Clean up stale E2E instances from a previous run (in case they were killed hard).
# Only touches the two dedicated test ports — never the user's dev server (1420).
command -v fuser >/dev/null && { fuser -k 45821/tcp 2>/dev/null; fuser -k 1421/tcp 2>/dev/null; }
sleep 0.5

TESTDIR=$(mktemp -d /tmp/hkd-e2e-XXXXXX)
echo "$TESTDIR" > /tmp/hkd-e2e-dir

export HKD_DATA_DIR="$TESTDIR"
export HKD_WEB_PORT=45821
export VITE_API_PROXY="http://127.0.0.1:45821"
export VITE_PORT=1421

# The tauri CLI waits for devUrl before launching the app — override it to Vite 1421
# via --config (leaves the user's 1420 alone). The app only starts once Vite 1421 is up.
# Runs dev in this very process (exec) so Playwright manages its lifecycle.
# Playwright waits for GET http://127.0.0.1:45821/api/health (the app's web server) to
# return 200 before starting tests — both app and Vite are ready, no startup race.
exec bun run tauri dev --config '{"build":{"devUrl":"http://localhost:1421"}}'