#!/usr/bin/env bash
# Whether a release `flayer` serves the panel from the binary itself, with no
# apps/web/dist on disk to read it from (#71):
#
#   scripts/serves-panel.sh target/x86_64-unknown-linux-gnu/release/flayer
#
# CI's `build (cli)` runs it, through `make cli/release/check`, on the release
# it built with the panel `test (web)` built. A release embeds each of the
# panel's files as it compiles, while a debug build reads the folder as each
# request comes in, so the folder has to be gone, or a binary reading it would
# pass for one that embeds it. It refuses to run while the folder is there
# rather than delete what somebody built.
#
# `flayer ui` is started as apps/cli/tests/ui_test.rs starts it: from a folder
# of its own, with a home of its own, nothing named or activated, every agent
# home empty and no browser opened, so it reads nothing of the machine's. Its
# link is the first line it prints. `/` has to answer with the panel's page,
# not the 503 a build without the panel answers, and every file under
# /assets/ that the page names has to be served too, so a binary holding the
# page and not what it loads does not pass. Then it is stopped with Ctrl-C,
# as a person stops it, and has to exit as a command that went well.
#
# It needs bash 4.4 or later, curl, and Unix for the Ctrl-C (SIGINT).
set -euo pipefail

if [ $# -ne 1 ]; then
	echo "usage: $0 <flayer binary>" >&2
	exit 2
fi
if [ ! -f "$1" ] || [ ! -x "$1" ]; then
	echo "error: $1 is not a binary that can run: build the release first" >&2
	exit 2
fi
# Absolute, since it is started from a folder of its own.
binary=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
dist=$(cd "$(dirname "$0")/.." && pwd)/apps/web/dist
if [ -e "$dist" ]; then
	echo "error: $dist is there: remove it first, or a binary that reads it would pass for one that embeds it" >&2
	exit 2
fi

scratch=$(mktemp -d)
pid=
cleanup() {
	if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
		kill -KILL "$pid" 2>/dev/null || true
		wait "$pid" 2>/dev/null || true
	fi
	rm -rf "$scratch"
}
trap cleanup EXIT

fail() {
	echo "error: $*" >&2
	if [ -s "$scratch/stderr" ]; then
		echo "flayer ui said on stderr:" >&2
		sed 's/^/  /' "$scratch/stderr" >&2
	fi
	exit 1
}

mkdir "$scratch/home" "$scratch/here"
(
	cd "$scratch/here"
	exec env -u MINDFLAYER_HOME -u MINDFLAYER_WORKSPACE -u CLAUDE_CONFIG_DIR \
		-u CODEX_HOME -u OPENCODE_CONFIG_DIR HOME="$scratch/home" \
		"$binary" ui --no-open --port 0 --home="$scratch/home" --workspace= \
		--claude-config-dir= --codex-home= --opencode-config-dir= --agents-dir=
) >"$scratch/stdout" 2>"$scratch/stderr" &
pid=$!

# The link is out as soon as the server listens, and the first line.
link=
for _ in $(seq 300); do
	if IFS= read -r link <"$scratch/stdout"; then
		break
	fi
	kill -0 "$pid" 2>/dev/null || fail "flayer ui stopped before it printed a link"
	sleep 0.1
done
case $link in
http://127.0.0.1:*/\?token=*) ;;
'') fail "flayer ui printed no link within 30 s" ;;
*) fail "the first line flayer ui printed is not a link: $link" ;;
esac
address=${link#http://}
address=${address%%/*}

status=$(curl -sS --max-time 10 -o "$scratch/page" -w '%{http_code}' "http://$address/") ||
	fail "nothing answered on http://$address/"
if [ "$status" != 200 ]; then
	fail "/ answered $status, not the panel: $(head -c 200 "$scratch/page")"
fi
if ! grep -q '<div id="app"></div>' "$scratch/page"; then
	fail "/ answered with a page that is not the panel's: $(head -c 200 "$scratch/page")"
fi

mapfile -t assets < <(grep -o '"/assets/[^"]*"' "$scratch/page" | tr -d '"' | sort -u)
case " ${assets[*]} " in
*.js\ *) ;;
*) fail "the panel's page names no script under /assets/" ;;
esac
for asset in "${assets[@]}"; do
	status=$(curl -sS --max-time 10 -o /dev/null -w '%{http_code}' "http://$address$asset") ||
		fail "nothing answered for $asset"
	[ "$status" = 200 ] || fail "$asset, which the panel's page names, answered $status"
done

kill -INT "$pid"
for _ in $(seq 300); do
	kill -0 "$pid" 2>/dev/null || break
	sleep 0.1
done
kill -0 "$pid" 2>/dev/null && fail "flayer ui still runs 30 s after Ctrl-C"
exited=0
wait "$pid" || exited=$?
pid=
[ "$exited" = 0 ] || fail "flayer ui exited with $exited after Ctrl-C"

echo "ok: $binary serves the panel from the binary: / and ${#assets[@]} files under /assets/"
