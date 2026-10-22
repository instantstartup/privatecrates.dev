#!/bin/sh
# Checks scripts/server-gate.sh without building anything: a fake cargo records what each call sees. Every call
# must build in the server's own target dir, with stdin closed and no prompt, whatever the caller exports.
set -u

root="$(cd "$(dirname "$0")/.." && pwd)"
dir=$(mktemp -d) || exit 1
trap 'rm -rf "$dir"' EXIT

cat >"$dir/cargo" <<'FAKE'
#!/bin/sh
stdin=open
[ -t 0 ] || read -r _ || stdin=closed
echo "$1 target=$CARGO_TARGET_DIR interactive=$PRIVATECRATES_INTERACTIVE stdin=$stdin" >>"$SERVER_GATE_TEST_LOG"
FAKE
chmod +x "$dir/cargo"

log="$dir/calls"
PATH="$dir:$PATH" SERVER_GATE_TEST_LOG="$log" CARGO_TARGET_DIR="$root/target" sh "$root/scripts/server-gate.sh" >/dev/null || {
	echo "server-gate-test: the script failed with a cargo that always succeeds" >&2
	exit 1
}

want="target=$root/crates/privatecrates-server/target interactive=0 stdin=closed"
expected="fetch $want
fmt $want
clippy $want
test $want"
if [ "$(cat "$log")" != "$expected" ]; then
	echo "server-gate-test: cargo calls differ. Expected:" >&2
	echo "$expected" >&2
	echo "Got:" >&2
	cat "$log" >&2
	exit 1
fi
echo "server-gate-test: ok"
