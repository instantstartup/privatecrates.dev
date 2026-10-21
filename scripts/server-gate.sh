#!/bin/sh
# The server workspace's gate, as CI's `server` job runs it (.github/workflows/ci.yml): fmt, clippy, tests.
# crates/privatecrates-server is its own workspace (it alone reads the private instantstartup registry), so the
# root workspace's build commands never reach it. It builds in its own target dir, crates/privatecrates-server/target.
#
# Never waits on a person: stdin is closed, the credential provider is told no one can answer a sign-in prompt,
# and every cargo call has a time limit. Fetching first separates "can't reach the registry or not signed in"
# from a build failure, and says how to sign in.
set -u

cd "$(dirname "$0")/../crates/privatecrates-server" || exit 1

export PRIVATECRATES_INTERACTIVE=0
export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-never}"

# Wall-clock limits in seconds; overridable for a slow machine.
fetch_limit="${SERVER_GATE_FETCH_TIMEOUT:-180}"
build_limit="${SERVER_GATE_BUILD_TIMEOUT:-2400}"

log=$(mktemp) || exit 1
trap 'rm -f "$log"' EXIT

echo "server: cargo fetch --locked"
timeout --kill-after=10 "$fetch_limit" cargo fetch --locked </dev/null >"$log" 2>&1
status=$?
if [ "$status" -ne 0 ]; then
	tail -n 30 "$log" >&2
	if [ "$status" -eq 124 ] || [ "$status" -eq 137 ]; then
		echo "server: fetching dependencies took over ${fetch_limit}s." >&2
	fi
	echo "server: can't fetch the server's dependencies from the instantstartup registry." >&2
	echo "server: if you are not signed in, sign in: cargo login --registry instantstartup" >&2
	exit 1
fi

run() {
	echo "server: $*"
	timeout --kill-after=10 "$build_limit" "$@" </dev/null
	status=$?
	if [ "$status" -eq 124 ] || [ "$status" -eq 137 ]; then
		echo "server: \`$*\` took over ${build_limit}s." >&2
	fi
	return "$status"
}

# The records tests need Postgres and are #[ignore]d, as in CI; its `postgres` job runs them.
run cargo fmt --check &&
	run cargo clippy --all-targets --locked -- -D warnings &&
	run cargo test --locked
