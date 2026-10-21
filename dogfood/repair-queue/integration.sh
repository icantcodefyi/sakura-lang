#!/bin/sh
# Installed-binary integration check for Repair Queue (BRIEF.md, last
# acceptance paragraph). `sakura test` has no host permissions, so this starts
# the real listener with `sakura run`, creates a ticket, restarts on the same
# SQLite file and reads it back, then checks that missing grants are refused.
#
# Usage: ./integration.sh   (needs `sakura` 1.0.0-beta.1 on PATH, curl, lsof; port 8787 free)
# Everything it writes stays under ./target/integration and is removed at the end.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
work="$here/target/integration"
sakura() { env -u ANTHROPIC_BASE_URL -u ANTHROPIC_AUTH_TOKEN command sakura "$@"; }
fail() { echo "FAIL: $*"; exit 1; }

# copy_project <dir> <permissions block>
copy_project() {
  mkdir -p "$1/data"
  cp -R "$here/src" "$here/sakura.lock" "$1/"
  printf '[package]\nname = "repair_queue"\nversion = "0.1.0"\nedition = "2026"\n\n[permissions]\n%b' "$2" > "$1/sakura.toml"
}

server_pid=""
start() {
  (cd "$1" && sakura run > "$1/server.log" 2>&1) &
  for _ in $(seq 1 100); do
    server_pid=$(lsof -nP -t -iTCP:8787 -sTCP:LISTEN 2>/dev/null || true)
    [ -n "$server_pid" ] && return 0
    sleep 0.1
  done
  cat "$1/server.log"; fail "listener did not start"
}
stop() {
  # Stop exactly the process listening on 8787 that this script started.
  [ -n "$server_pid" ] && kill "$server_pid" && wait 2>/dev/null || true
  server_pid=""
}
trap 'stop; rm -rf "$work"' EXIT

lsof -nP -iTCP:8787 -sTCP:LISTEN >/dev/null 2>&1 && fail "port 8787 is already in use"
rm -rf "$work"
grants='fs.read = ["./data"]\nfs.write = ["./data"]\nnet = ["127.0.0.1:8787"]\n'

echo "== persistence across a restart"
app="$work/app"
copy_project "$app" "$grants"
start "$app"
code=$(curl -s -o "$work/create.json" -w '%{http_code}' -X POST http://127.0.0.1:8787/tickets \
  -d '{"id":"FAN-1","description":"Desk fan rattles"}')
[ "$code" = 201 ] || fail "create returned $code"
code=$(curl -s -o /dev/null -w '%{http_code}' -X PUT http://127.0.0.1:8787/tickets/FAN-1/state \
  -d '{"state":"in-progress","expected":"waiting"}')
[ "$code" = 200 ] || fail "transition returned $code"
stop
[ -f "$app/data/repair-queue.sqlite" ] || fail "no database file under ./data"
start "$app"
grep -q "0 migration steps applied), 1 tickets" "$app/server.log" || fail "restart did not reuse the schema: $(cat "$app/server.log")"
body=$(curl -s http://127.0.0.1:8787/tickets/FAN-1)
expected='{"id":"FAN-1","description":"Desk fan rattles","station":"intake","state":"in-progress","history":[{"seq":1,"from":"waiting","to":"in-progress"}]}'
[ "$body" = "$expected" ] || fail "after restart got $body"
# check <expected status> <curl args...>
check() {
  want=$1; shift
  got=$(curl -s -o /dev/null -w '%{http_code}' "$@")
  [ "$got" = "$want" ] || fail "curl $* returned $got, expected $want"
}
check 404 http://127.0.0.1:8787/nowhere
check 404 http://127.0.0.1:8787/tickets/NOPE
check 405 -X DELETE http://127.0.0.1:8787/tickets
check 400 http://127.0.0.1:8787/tickets/bad%27id
check 400 -X POST http://127.0.0.1:8787/tickets -d '{"id":"X"}'
check 409 -X POST http://127.0.0.1:8787/tickets -d '{"id":"FAN-1","description":"again"}'
check 409 -X PUT http://127.0.0.1:8787/tickets/FAN-1/state -d '{"state":"withdrawn","expected":"waiting"}'
check 200 -X PUT http://127.0.0.1:8787/tickets/FAN-1/station -d '{"station":"electrical"}'
check 200 http://127.0.0.1:8787/tickets
stop
echo "ok: ticket survived the restart: $body"

# expect_refused <name> <permissions block> <expected exit>
# A run that is not refused would serve forever, so it is stopped after 10 s
# and reported as a failure.
expect_refused() {
  dir="$work/$1"
  copy_project "$dir" "$2"
  (cd "$dir" && { sakura run > "$dir/run.log" 2>&1 && echo 0 || echo $?; } > "$dir/status.tmp" \
    && mv "$dir/status.tmp" "$dir/status") &
  for _ in $(seq 1 100); do
    [ -f "$dir/status" ] && break
    sleep 0.1
  done
  if [ ! -f "$dir/status" ]; then
    server_pid=$(lsof -nP -t -iTCP:8787 -sTCP:LISTEN 2>/dev/null || true)
    stop
    fail "$1 was not refused: $(cat "$dir/run.log")"
  fi
  status=$(cat "$dir/status")
  [ "$status" = "$3" ] || { cat "$dir/run.log"; fail "$1 exited $status, expected $3"; }
  echo "ok: $1 refused with exit $status: $(head -c 300 "$dir/run.log" | tr '\n' ' ')"
}

echo "== refused grants"
expect_refused no-net 'fs.read = ["./data"]\nfs.write = ["./data"]\n' 75
expect_refused no-fs 'net = ["127.0.0.1:8787"]\n' 75
expect_refused other-host 'fs.read = ["./data"]\nfs.write = ["./data"]\nnet = ["localhost:8787"]\n' 1
expect_refused fs-read-only 'fs.read = ["./data"]\nnet = ["127.0.0.1:8787"]\n' 1
echo "all integration checks passed"
