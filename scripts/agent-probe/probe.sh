#!/usr/bin/env bash
# Asks the coding agents themselves where they read skills and rules (#17), so
# that the adapters build on what the binaries do rather than on what their
# documentation says. docs/agents.md records each run: the version, the
# environment, the commands and what every agent answered.
#
#   scripts/agent-probe/probe.sh [claude]
#
# It runs the agents against their APIs with the operator's own login, which is
# why it is run by hand and never by CI or `make test`. Only Claude Code has
# checks so far. An agent gets them from a file beside this one, <agent>.sh,
# defining <agent>_prepare, run out here where the real home is visible, and
# <agent>_checks, run inside the namespace below (see claude.sh).
#
# Linux only. It needs bubblewrap (bwrap) on PATH, jq and git in /usr/bin or
# /bin, the only PATH inside the namespace, and each agent's own binary
# rather than a script or shim that would run something the namespace hides.
#
# Isolation. An agent reads folders in the user's home whatever its own
# variable says: Codex and OpenCode read ~/.agents/skills, which on the owner's
# machine already holds other tools' skills. So pointing CLAUDE_CONFIG_DIR and
# the rest elsewhere is not enough on its own. The run lives in one temp dir,
# target/agent-probe-run, deleted when the script exits however it exits, and
# every agent starts in a bubblewrap mount namespace where:
#
# - the real home is an empty tmpfs, so nothing in it can be read or written,
#   whichever way an agent looks for it (HOME, the passwd entry, a path it
#   spells itself);
# - the temp dir is mounted at /mnt/agent-probe, and HOME, the XDG_*_HOME
#   variables, CLAUDE_CONFIG_DIR, CODEX_HOME and OPENCODE_CONFIG_DIR all point
#   into it;
# - the environment is cleared, so nothing of the session that launched the
#   script (a Claude Code session sets a dozen CLAUDE_CODE_* variables) reaches
#   the agent;
# - /tmp is a private tmpfs, gone when the agent exits.
#
# Mounting the temp dir at /mnt/agent-probe, rather than running it where it
# sits, is what makes the checks about parent folders possible at all. Claude
# reads CLAUDE.md and AGENTS.md from every folder above where it starts, and
# above target/ sit this checkout's own AGENTS.md and, in a linked worktree,
# the main checkout's CLAUDE.md, which alone would stop Claude reading any
# AGENTS.md. Under /mnt/agent-probe the only folders above a scenario are ones
# the script made, and each check asserts as much before it runs.
#
# Before and after the run it fingerprints the user-scope folders an agent
# could write to (~/.agents, ~/.codex, ~/.claude/skills, ~/.claude/rules,
# ~/.claude/CLAUDE.md and ~/.claude.json) and says whether they changed. Not
# the whole ~/.claude: a Claude Code session running this script writes there
# itself.

set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
mount_root=/mnt/agent-probe
mount_scripts=/mnt/agent-probe-scripts
namespace_path=/usr/bin:/bin
known_agents=(claude codex opencode)

die() {
	printf 'agent-probe: %s\n' "$*" >&2
	exit 2
}

say() { printf '%s\n' "$*"; }

in_list() {
	local needle=$1 item
	shift
	for item in "$@"; do [[ $item == "$needle" ]] && return 0; done
	return 1
}

agents_from() {
	agents=("$@")
	[[ ${#agents[@]} -gt 0 ]] || agents=(claude)
	local agent
	for agent in "${agents[@]}"; do
		in_list "$agent" "${known_agents[@]}" ||
			die "unknown agent '$agent' (${known_agents[*]})"
		[[ -f $here/$agent.sh ]] ||
			die "no $agent checks yet: docs/agents.md marks its facts docs-only"
	done
}

# One line per path: absent, or a hash of what it holds. Names and content,
# not times, so a folder another program merely opened still reads as
# untouched, and a file rewritten with other bytes does not.
fingerprint() {
	local path hash
	for path in "$@"; do
		if [[ -d $path ]]; then
			hash=$({
				find -H "$path" -mindepth 1 -printf '%P %y %l\n' | LC_ALL=C sort
				find -H "$path" -type f -print0 | LC_ALL=C sort -z | xargs -0r sha256sum
			} 2>&1 | sha256sum | cut -d' ' -f1)
			printf '%s dir %s\n' "$path" "$hash"
		elif [[ -e $path ]]; then
			printf '%s file %s\n' "$path" "$(sha256sum <"$path" | cut -d' ' -f1)"
		else
			printf '%s absent\n' "$path"
		fi
	done
}

outside() {
	agents_from "$@"
	local tool
	for tool in bwrap sha256sum; do
		command -v "$tool" >/dev/null || die "needs $tool on PATH"
	done
	# The checks run jq and git inside the namespace, where the real home is
	# hidden. One found only in ~/.local/bin or a version manager's folder
	# would pass out here and be missing in there, and every answer would
	# then come out empty, as if the agent had loaded nothing.
	for tool in jq git; do
		[[ -x /usr/bin/$tool || -x /bin/$tool ]] ||
			die "needs $tool in /usr/bin or /bin, the only PATH inside the namespace"
	done
	[[ -n ${HOME:-} && -d $HOME ]] || die "HOME is not a directory"
	real_home=$(cd "$HOME" && pwd -P)
	local repo
	repo=$(cd "$here/../.." && pwd -P)
	run=$repo/target/agent-probe-run

	# A run that was killed leaves its temp dir, credentials copy included.
	# The marker says it is one of ours to delete; anything else at that path
	# is somebody's, and is left alone.
	if [[ -e $run ]]; then
		[[ -f $run/.agent-probe ]] || die "$run exists and is not a probe run"
		rm -rf -- "$run"
	fi
	mkdir -p -- "$run/home" "$run/stage" "$run/results"
	: >"$run/.agent-probe"
	trap 'rm -rf -- "$run"' EXIT
	trap 'exit 130' INT TERM

	local watched=(
		"$real_home/.agents"
		"$real_home/.codex"
		"$real_home/.claude/skills"
		"$real_home/.claude/rules"
		"$real_home/.claude/CLAUDE.md"
		"$real_home/.claude.json"
	)
	local before after
	before=$(fingerprint "${watched[@]}")

	# Each agent's prepare copies in its login and names the binaries the
	# namespace must see and the variables it must carry.
	expose=()
	namespace_env=()
	local agent
	for agent in "${agents[@]}"; do
		# shellcheck source=/dev/null
		source "$here/$agent.sh"
		"${agent}_prepare"
	done

	local args=(
		--die-with-parent
		--new-session
		--dev-bind / /
		--tmpfs /tmp
		--tmpfs "$real_home"
		--tmpfs /mnt
		--bind "$run" "$mount_root"
		--ro-bind "$here" "$mount_scripts"
	)
	local bin
	for bin in "${expose[@]}"; do args+=(--ro-bind "$bin" "$bin"); done
	local env=(
		HOME "$mount_root/home"
		XDG_CONFIG_HOME "$mount_root/home/.config"
		XDG_DATA_HOME "$mount_root/home/.local/share"
		XDG_STATE_HOME "$mount_root/home/.local/state"
		XDG_CACHE_HOME "$mount_root/home/.cache"
		CLAUDE_CONFIG_DIR "$mount_root/home/.claude"
		CODEX_HOME "$mount_root/home/.codex"
		OPENCODE_CONFIG_DIR "$mount_root/home/.config/opencode"
		PATH "$namespace_path"
		LANG C.UTF-8
		PROBE_REAL_HOME "$real_home"
		"${namespace_env[@]}"
	)
	args+=(--clearenv)
	local i
	for ((i = 0; i < ${#env[@]}; i += 2)); do
		args+=(--setenv "${env[i]}" "${env[i + 1]}")
	done
	args+=(--chdir "$mount_root")

	say "agent-probe: $(date -u +%Y-%m-%dT%H:%M:%SZ), $(uname -sr), $(bwrap --version), $(env PATH="$namespace_path" git --version)"
	say "namespace environment (cleared first):"
	for ((i = 0; i < ${#env[@]}; i += 2)); do say "  ${env[i]}=${env[i + 1]}"; done

	local status=0
	bwrap "${args[@]}" /bin/bash "$mount_scripts/probe.sh" --inside "${agents[@]}" || status=$?
	for agent in "${agents[@]}"; do
		if declare -F "${agent}_finish" >/dev/null; then "${agent}_finish"; fi
	done

	after=$(fingerprint "${watched[@]}")
	say ""
	say "real home, before and after:"
	if [[ $before == "$after" ]]; then
		sed 's/^/  unchanged  /' <<<"$after"
	else
		diff <(printf '%s\n' "$before") <(printf '%s\n' "$after") | sed 's/^/  /' || true
		status=3
	fi

	local kept
	kept=$repo/target/agent-probe-results/$(date -u +%Y%m%dT%H%M%SZ)
	mkdir -p -- "$kept"
	cp -R -- "$run/results/." "$kept/"
	say "results kept in $kept"
	exit "$status"
}

# Instruction files above the scenario, which every agent here reads by
# walking up, must be the ones the scenario put there.
assert_ancestry() {
	local dir=$1 file
	shift
	while :; do
		for file in CLAUDE.md CLAUDE.local.md .claude/CLAUDE.md AGENTS.md \
			.claude/AGENTS.md AGENTS.override.md; do
			[[ -e $dir/$file ]] || continue
			in_list "$dir/$file" "$@" ||
				die "an instruction file the scenario did not write is above it: $dir/$file"
		done
		[[ $dir == / ]] && break
		dir=$(dirname "$dir")
	done
}

# Nothing at user scope may answer for a project: a skill, a rule or an
# instruction file in the temp home is read in every scenario, and would blur
# what the project's own files did. A scenario about user scope names the one
# file it put there.
assert_clean_home() {
	local path
	for path in "$HOME/.claude/skills" "$HOME/.claude/rules" \
		"$HOME/.claude/CLAUDE.md" "$HOME/.claude/AGENTS.md" "$HOME/.agents" \
		"$HOME/.codex/AGENTS.md" "$HOME/.codex/AGENTS.override.md" \
		"$HOME/.codex/skills" "$HOME/.config/opencode/AGENTS.md" \
		"$HOME/.config/opencode/skills" "$HOME/CLAUDE.md" "$HOME/AGENTS.md"; do
		in_list "$path" "$@" && continue
		[[ ! -e $path ]] || die "user scope in the temp home is not empty: $path"
	done
}

# A fresh repository, made with no config but its own, so the operator's
# templates and hooks stay out of it.
new_repo() {
	mkdir -p -- "$1"
	GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null git init -q -- "$1"
}

put() {
	mkdir -p -- "$(dirname "$1")"
	printf '%s\n' "$2" >"$1"
}

# Excludes paths the way Mindflayer's local mode does: anchored entries in a
# marked block of $GIT_COMMON_DIR/info/exclude.
exclude() {
	local repo=$1 file
	shift
	file=$(git -C "$repo" rev-parse --git-common-dir)/info/exclude
	[[ $file == /* ]] || file=$repo/$file
	mkdir -p -- "$(dirname "$file")"
	{
		printf '# mindflayer:begin\n# project .\n'
		printf '%s\n' "$@"
		printf '# mindflayer:end\n'
	} >>"$file"
}

# What git itself says is ignored, so a check that relies on a path being
# excluded shows that it was.
show_ignored() {
	local repo=$1 name=$2
	{
		printf '$ git status --porcelain --ignored --untracked-files=all\n'
		git -C "$repo" status --porcelain --ignored --untracked-files=all
		printf '$ git check-ignore -v --no-index <each excluded path>\n'
		shift 2
		local path
		for path in "$@"; do
			git -C "$repo" check-ignore -v --no-index -- "$path" || printf 'not ignored: %s\n' "$path"
		done
	} | tee "$results/$name.git.txt" | sed 's/^/  /'
}

verdict() {
	local name=$1 want=$2 got=$3 subject=$4 why=$5 status=ok
	if [[ $want != "$got" ]]; then
		status=MISMATCH
		mismatches=$((mismatches + 1))
	fi
	printf '%-8s %-22s %-28s want %-7s got %-7s %s\n' \
		"$status" "$name" "$subject" "$want" "$got" "$why" | tee -a "$results/summary.txt"
}

inside() {
	agents_from "$@"
	# The guard every check relies on: refuse unless HOME is the temp home and
	# the real one is out of sight.
	[[ -f $mount_root/.agent-probe ]] || die "not inside a probe run; start probe.sh without --inside"
	[[ ${HOME:-} == "$mount_root/home" ]] || die "HOME is '${HOME:-}', not $mount_root/home: refusing to run"
	[[ -n ${PROBE_REAL_HOME:-} ]] || die "PROBE_REAL_HOME is not set"
	local path
	for path in .agents .codex .claude .claude.json; do
		[[ ! -e $PROBE_REAL_HOME/$path ]] || die "the real home is visible at $PROBE_REAL_HOME: refusing to run"
	done

	results=$mount_root/results
	stage=$mount_root/stage
	mismatches=0
	local agent
	for agent in "${agents[@]}"; do
		# shellcheck source=/dev/null
		source "$mount_scripts/$agent.sh"
		"${agent}_checks"
	done
	say ""
	say "summary:"
	sed 's/^/  /' "$results/summary.txt"
	((mismatches == 0)) || die "$mismatches result(s) differ from what docs/agents.md expects"
}

if [[ ${1:-} == --inside ]]; then
	shift
	inside "$@"
else
	outside "$@"
fi
