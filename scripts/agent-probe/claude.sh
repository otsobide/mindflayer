# Claude Code's checks for probe.sh, which sources this file: claude_prepare
# runs outside the namespace, where the real home is visible, and
# claude_checks inside it. docs/agents.md says what each check found.
#
# Every file under test carries a token of its own (PROBE-AGENTS-MD-1, ...),
# and each run asks Claude, once, which tokens it holds and which skills it can
# invoke. A token in the answer means that file reached the model; a skill in
# the session's init message means Claude loaded it. The InstructionsLoaded
# hook adds a record of which instruction files loaded and why that does not
# depend on the model's answer; Claude's docs say an AGENTS.md read natively
# does not fire it, so for AGENTS.md the answer is the evidence.
#
# Runs use haiku, short prompts and a budget cap. A run that lists keeps only
# the Skill tool, since skills are part of what it is asked about, and nothing
# that reads a file; the one run that has to touch a file gets Read alone.
# Each run is also held to the tools it was meant to call and the turns that
# take: an answer the model put together by reading AGENTS.md would say
# nothing about what Claude loaded.

claude_prepare() {
	local bin=${PROBE_CLAUDE_BIN:-}
	if [[ -z $bin ]]; then
		bin=$(command -v claude) || die "no claude on PATH (or set PROBE_CLAUDE_BIN)"
	fi
	bin=$(readlink -f -- "$bin")
	# A wrapper script would exec something under the real home, which the
	# namespace hides.
	[[ $(head -c 2 -- "$bin") != '#!' ]] ||
		die "$bin is a wrapper script: set PROBE_CLAUDE_BIN to the claude binary itself"
	# A version manager's shim is no script: mise's is a link to mise itself,
	# which would then answer every run as mise. Asking the binary what it is
	# catches any such stand-in, where a list of known shims would not.
	local version
	version=$("$bin" --version 2>/dev/null | head -n 1) || version=
	[[ $version == *'(Claude Code)'* ]] ||
		die "$bin is not Claude Code (its --version says '${version:-nothing}'): set PROBE_CLAUDE_BIN to the binary itself, as \`mise which claude\` prints it"

	# Only the login, never the rest of the folder: the operator's settings,
	# skills, rules, CLAUDE.md and plugins are exactly what must not take part.
	local login=${CLAUDE_CONFIG_DIR:-$real_home/.claude}/.credentials.json
	[[ -f $login ]] || die "no Claude login at $login: run claude and log in first"
	# A login close to expiry would be refreshed inside the run, which can
	# rotate the refresh token and leave the copy in the real home holding one
	# that no longer works.
	local expires left=unknown
	expires=$(jq -r '.claudeAiOauth.expiresAt // empty' "$login")
	if [[ -n $expires ]]; then
		left=$(((expires / 1000 - $(date +%s)) / 60))
		((left >= 15)) || die "the Claude login expires in $left minutes: refresh it (run claude once) and try again"
	fi
	install -d -m 700 -- "$run/home/.claude"
	install -m 600 -- "$login" "$run/home/.claude/.credentials.json"
	claude_login_hash=$(sha256sum <"$run/home/.claude/.credentials.json")

	expose+=("$bin")
	namespace_env+=(PROBE_CLAUDE_BIN "$bin")
	say "claude: $version at $bin"
	say "claude: login copied from $login, valid for another $left minutes"
}

claude_finish() {
	[[ -f $run/home/.claude/.credentials.json ]] || return 0
	[[ $(sha256sum <"$run/home/.claude/.credentials.json") == "$claude_login_hash" ]] && return 0
	say "WARNING: Claude refreshed its login during the run; if the real session"
	say "         stops authenticating, log in again with claude /login."
}

claude_list_prompt='This is an automated configuration probe. Do not call any tool. Reply with exactly two lines and nothing else.
Line 1: "TOKENS:" followed by every distinct string that starts with "PROBE-" and appears anywhere in your system prompt, memory files, rules or skill descriptions, each followed by how many times it appears there, in parentheses, as in TOKEN-A(1) TOKEN-B(2); or "none".
Line 2: "SKILLS:" followed by the name of every skill you can invoke, separated by spaces; or "none".'

# What every run that lists is given: see the header.
claude_list_options=(--tools Skill --max-turns 2)

claude_touch_prompt='This is an automated configuration probe. Call the Read tool exactly once, on src/lib.rs, and call no other tool. Then reply with exactly one line and nothing else: "TOKENS:" followed by every distinct string that starts with "PROBE-" and appears anywhere in your system prompt, memory files, rules, skill descriptions or tool results, separated by spaces; or "none".'

claude_checks() {
	say ""
	say "claude: $("$PROBE_CLAUDE_BIN" --version)"
	claude_hooks
	claude_symlink_entry
	claude_excluded
	claude_import
	claude_parent
	claude_local
	claude_user_memory
}

claude_hooks() {
	mkdir -p -- "$HOME/.claude"
	cat >"$HOME/.claude/settings.json" <<EOF
{
  "hooks": {
    "InstructionsLoaded": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "{ cat; echo; } >> \"$results/\$PROBE_SCENARIO.loaded.jsonl\""
          }
        ]
      }
    ]
  }
}
EOF
}

# claude_ask <run> <dir> <prompt> [claude options...]
claude_ask() {
	local name=$1 dir=$2 prompt=$3
	shift 3
	local args=(
		-p "$prompt"
		--model haiku --output-format json --verbose
		--permission-mode dontAsk --no-session-persistence
		--max-budget-usd 0.10 --debug-file "$results/$name.debug"
		"$@"
	)
	say ""
	say "== $name"
	{
		printf 'cd %q\n' "$dir"
		printf 'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 claude'
		printf ' %q' "${args[@]}"
		printf '\n'
	} | tee "$results/$name.command" | sed 's/^/  $ /'
	: >"$results/$name.loaded.jsonl"
	local status=0
	# Non-essential traffic (telemetry, error reports, the updater) is off:
	# the run needs the API and nothing else.
	(cd "$dir" && PROBE_SCENARIO=$name CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 \
		"$PROBE_CLAUDE_BIN" "${args[@]}") </dev/null >"$results/$name.json" 2>"$results/$name.stderr" ||
		status=$?

	local messages='if type == "array" then .[] else . end'
	jq -r "$messages | select(.type == \"result\") | .result // empty" \
		"$results/$name.json" >"$results/$name.answer" 2>/dev/null || true
	jq -r "[$messages | select(.type == \"system\" and .subtype == \"init\")][0]
		| (.skills // .slash_commands // [])[]
		| if type == \"object\" then .name else . end" \
		"$results/$name.json" >"$results/$name.skills" 2>/dev/null || true
	jq -r "$messages | select(.type == \"assistant\") | .message.content[]?
		| select(.type == \"tool_use\") | .name" \
		"$results/$name.json" >"$results/$name.tools" 2>/dev/null || true
	jq -r "$messages | select(.type == \"result\") | .num_turns // empty" \
		"$results/$name.json" >"$results/$name.turns" 2>/dev/null || true
	local facts
	facts=$(jq -r "$messages | select(.type == \"result\")
		| \"exit $status, \(.num_turns) turn(s), \(.total_cost_usd) USD, models \(.modelUsage // {} | keys | join(\",\"))\"" \
		"$results/$name.json" 2>/dev/null || true)
	say "  ${facts:-exit $status, no result message}"
	if [[ ! -s $results/$name.answer ]]; then
		say "  no answer; stderr:"
		head -n 20 -- "$results/$name.stderr" | sed 's/^/    /'
	fi
	say "  answer:"
	sed 's/^/    /' "$results/$name.answer"
	say "  skills in the init message: $(tr '\n' ' ' <"$results/$name.skills")"
	say "  tools called: $(tr '\n' ' ' <"$results/$name.tools")"
	say "  InstructionsLoaded:"
	jq -r 'select(.file_path != null)
		| "    \(.load_reason) \(.memory_type) \(.file_path)"' \
		"$results/$name.loaded.jsonl" 2>/dev/null || true
	# The debug log is kept only for its lines about instruction files and
	# skills; the rest is about the session and the account.
	grep -aiE 'claude\.md|claude\.local\.md|agents\.md|rules|skill|memory|instruction' \
		-- "$results/$name.debug" >"$results/$name.debug.txt" 2>/dev/null || true
	rm -f -- "$results/$name.debug"
}

claude_tokens() {
	grep -oE 'PROBE-[A-Z0-9]+(-[A-Z0-9]+)*' -- "$results/$1.answer" | sort -u
}

# expect_token <run> seen|unseen <token> <what it shows>
expect_token() {
	local got=unseen
	claude_tokens "$1" | grep -qx -- "$3" && got=seen
	verdict "$1" "$2" "$got" "$3" "$4"
}

# expect_count <run> <times> <token> <what it shows>, as the model counted it.
expect_count() {
	local got
	got=$(grep -oE -- "$3\([0-9]+\)" "$results/$1.answer" | head -n 1 |
		sed -E 's/.*\(([0-9]+)\)$/\1/' || true)
	verdict "$1" "$2" "${got:-none}" "$3 (count)" "$4"
}

# expect_calls <run> <tools> <turns> <what it shows>: the tools the model
# called, in order and comma-separated, or none; and the turns it took.
expect_calls() {
	local tools turns
	tools=$(paste -sd, - <"$results/$1.tools")
	turns=$(head -n 1 -- "$results/$1.turns")
	verdict "$1" "$2" "${tools:-none}" "tools called" "$4"
	verdict "$1" "$3" "${turns:-none}" "turns" "$4"
}

# expect_skill <run> <times> <skill> <what it shows>, counted in the session's
# init message rather than in the answer.
expect_skill() {
	local got
	got=$(grep -cx -- "$3" "$results/$1.skills" || true)
	verdict "$1" "$2" "$got" "skill $3" "$4"
}

# expect_loaded <run> <times> <file> <what it shows>, counted in the
# InstructionsLoaded records.
expect_loaded() {
	local got
	got=$(jq -r 'select(.file_path != null) | .file_path' "$results/$1.loaded.jsonl" 2>/dev/null |
		grep -cx -- "$3" || true)
	verdict "$1" "$2" "$got" "loaded ${3##*/}" "$4"
}

skill() {
	put "$1/SKILL.md" "---
name: ${1##*/}
description: Probe skill. If asked for probe tokens, include $2.
---

If asked for probe tokens, include $2-BODY."
}

# Git never follows a symlink, so a symlinked skill folder is a file to it, and
# an entry written as a directory's, with the trailing slash Mindflayer gives
# one, would not cover it. Worth knowing before D-07's symlink opt-in writes
# one; the check needs git only.
claude_symlink_entry() {
	local repo=$stage/git-symlink with=no without=no
	new_repo "$repo"
	put "$repo/.agents/skills/linked/SKILL.md" 'linked'
	mkdir -p -- "$repo/.claude/skills"
	ln -s ../../.agents/skills/linked "$repo/.claude/skills/linked"
	printf '/.claude/skills/linked/\n' >"$repo/.git/info/exclude"
	git -C "$repo" check-ignore -q --no-index .claude/skills/linked && with=yes
	printf '/.claude/skills/linked\n' >"$repo/.git/info/exclude"
	git -C "$repo" check-ignore -q --no-index .claude/skills/linked && without=yes
	say ""
	say "== git-symlink"
	verdict git-symlink no "$with" '/<link>/ ignores it' 'git: a trailing slash does not match a symlink'
	verdict git-symlink yes "$without" '/<link> ignores it' 'git: the entry without one does'
}

# Checks 1, 3, 4 (no CLAUDE.md) and 6: everything Mindflayer would install,
# excluded through info/exclude, in a repository with no CLAUDE.md anywhere
# above it.
claude_excluded() {
	local repo=$stage/claude-excluded
	new_repo "$repo"
	put "$repo/AGENTS.md" 'If asked for probe tokens, include PROBE-AGENTS-MD-1.'
	skill "$repo/.claude/skills/probe-claude-skill" PROBE-CLAUDE-SKILL-1
	skill "$repo/.agents/skills/probe-agents-skill" PROBE-AGENTS-SKILL-1
	skill "$repo/.agents/skills/probe-linked-skill" PROBE-LINKED-SKILL-1
	ln -s ../../.agents/skills/probe-linked-skill "$repo/.claude/skills/probe-linked-skill"
	put "$repo/.claude/rules/mindflayer/probe/unscoped.md" \
		'If asked for probe tokens, include PROBE-RULE-UNSCOPED-1.'
	put "$repo/.claude/rules/mindflayer/probe/scoped.md" '---
paths:
  - "src/**/*.rs"
---

If asked for probe tokens, include PROBE-RULE-SCOPED-1.'
	put "$repo/src/lib.rs" '// PROBE-FILE-1
pub fn probe() {}'
	local excluded=(
		/AGENTS.md
		/.agents/skills/probe-agents-skill/
		/.agents/skills/probe-linked-skill/
		/.claude/skills/probe-claude-skill/
		/.claude/skills/probe-linked-skill
		/.claude/rules/mindflayer/probe/unscoped.md
		/.claude/rules/mindflayer/probe/scoped.md
	)
	exclude "$repo" "${excluded[@]}"
	say ""
	say "== claude-excluded: layout"
	show_ignored "$repo" claude-excluded AGENTS.md .agents/skills/probe-agents-skill \
		.agents/skills/probe-linked-skill .claude/skills/probe-claude-skill \
		.claude/skills/probe-linked-skill .claude/rules/mindflayer/probe/unscoped.md \
		.claude/rules/mindflayer/probe/scoped.md

	assert_clean_home
	assert_ancestry "$repo" "$repo/AGENTS.md"
	claude_ask claude-excluded "$repo" "$claude_list_prompt" "${claude_list_options[@]}"
	local run=claude-excluded rules=$repo/.claude/rules/mindflayer/probe
	expect_calls $run none 1 'the answer comes from what Claude loaded, not a tool'
	expect_skill $run 1 probe-claude-skill 'check 1: an excluded .claude/skills skill loads'
	expect_skill $run 0 probe-agents-skill 'check 1: a skill only in .agents/skills does not'
	expect_skill $run 1 probe-linked-skill 'check 3: an excluded relative symlink loads, once'
	expect_token $run seen PROBE-CLAUDE-SKILL-1 'check 1: its description reaches the model'
	expect_token $run unseen PROBE-AGENTS-SKILL-1 'check 1: the .agents-only one does not'
	expect_token $run seen PROBE-LINKED-SKILL-1 'check 3: the symlinked one does'
	expect_token $run seen PROBE-AGENTS-MD-1 'checks 1, 4: an excluded AGENTS.md is read when no CLAUDE.md exists'
	expect_token $run seen PROBE-RULE-UNSCOPED-1 'check 6: an excluded rule without paths: loads at start'
	expect_token $run unseen PROBE-RULE-SCOPED-1 'check 6: a rule with paths: does not, at start'
	expect_loaded $run 1 "$rules/unscoped.md" 'check 6: the hook saw the rule without paths: once'
	expect_loaded $run 0 "$rules/scoped.md" 'check 6: and not the one with paths:'

	assert_clean_home
	assert_ancestry "$repo" "$repo/AGENTS.md"
	claude_ask claude-touch "$repo" "$claude_touch_prompt" \
		--tools Read --allowedTools Read --max-turns 3
	run=claude-touch
	expect_calls $run Read 2 'check 6: one Read of src/lib.rs, then the answer'
	expect_token $run seen PROBE-FILE-1 'check 6: Claude read src/lib.rs'
	expect_token $run seen PROBE-RULE-SCOPED-1 'check 6: the rule with paths: loads once a matching file is read'
	expect_loaded $run 1 "$rules/scoped.md" 'check 6: the hook saw it load'
}

# Check 4: a CLAUDE.md that imports AGENTS.md loads it once.
claude_import() {
	local repo=$stage/claude-import
	new_repo "$repo"
	put "$repo/CLAUDE.md" '@AGENTS.md

If asked for probe tokens, include PROBE-CLAUDE-MD-2.'
	put "$repo/AGENTS.md" 'If asked for probe tokens, include PROBE-AGENTS-MD-2.'
	assert_clean_home
	assert_ancestry "$repo" "$repo/CLAUDE.md" "$repo/AGENTS.md"
	claude_ask claude-import "$repo" "$claude_list_prompt" "${claude_list_options[@]}"
	local run=claude-import
	expect_calls $run none 1 'the answer comes from what Claude loaded, not a tool'
	expect_token $run seen PROBE-CLAUDE-MD-2 'check 4: the CLAUDE.md is read'
	expect_token $run seen PROBE-AGENTS-MD-2 'check 4: AGENTS.md is read through @AGENTS.md'
	expect_count $run 1 PROBE-AGENTS-MD-2 'check 4: once, not twice'
	expect_loaded $run 1 "$repo/AGENTS.md" 'check 4: the hook saw AGENTS.md load once'
}

# Check 4: a CLAUDE.md in a folder above the git root, as ~/Work would hold,
# stops Claude reading the repository's AGENTS.md.
claude_parent() {
	local above=$stage/claude-parent
	local repo=$above/repo
	put "$above/CLAUDE.md" 'If asked for probe tokens, include PROBE-PARENT-CLAUDE-MD-3.'
	new_repo "$repo"
	put "$repo/AGENTS.md" 'If asked for probe tokens, include PROBE-AGENTS-MD-3.'
	assert_clean_home
	assert_ancestry "$repo" "$above/CLAUDE.md" "$repo/AGENTS.md"
	claude_ask claude-parent "$repo" "$claude_list_prompt" "${claude_list_options[@]}"
	local run=claude-parent
	expect_calls $run none 1 'the answer comes from what Claude loaded, not a tool'
	expect_token $run seen PROBE-PARENT-CLAUDE-MD-3 'check 4: the CLAUDE.md above the git root is read'
	expect_token $run unseen PROBE-AGENTS-MD-3 'check 4: and AGENTS.md is not'
}

# D-03: a CLAUDE.local.md alone, excluded as it usually is, also stops Claude
# reading AGENTS.md, which is why Mindflayer never creates one.
claude_local() {
	local repo=$stage/claude-local
	new_repo "$repo"
	put "$repo/CLAUDE.local.md" 'If asked for probe tokens, include PROBE-CLAUDE-LOCAL-MD-4.'
	put "$repo/AGENTS.md" 'If asked for probe tokens, include PROBE-AGENTS-MD-4.'
	exclude "$repo" /CLAUDE.local.md /AGENTS.md
	assert_clean_home
	assert_ancestry "$repo" "$repo/CLAUDE.local.md" "$repo/AGENTS.md"
	claude_ask claude-local "$repo" "$claude_list_prompt" "${claude_list_options[@]}"
	local run=claude-local
	expect_calls $run none 1 'the answer comes from what Claude loaded, not a tool'
	expect_token $run seen PROBE-CLAUDE-LOCAL-MD-4 'D-03: CLAUDE.local.md is read'
	expect_token $run unseen PROBE-AGENTS-MD-4 'D-03: and AGENTS.md is not'
}

# #44: the user memory file does not count as a CLAUDE.md for the AGENTS.md
# check. The one scenario that puts a file at user scope, and removes it.
claude_user_memory() {
	local repo=$stage/claude-user-memory memory=$HOME/.claude/CLAUDE.md
	new_repo "$repo"
	put "$repo/AGENTS.md" 'If asked for probe tokens, include PROBE-AGENTS-MD-5.'
	put "$memory" 'If asked for probe tokens, include PROBE-USER-CLAUDE-MD-5.'
	assert_clean_home "$memory"
	assert_ancestry "$repo" "$repo/AGENTS.md"
	claude_ask claude-user-memory "$repo" "$claude_list_prompt" "${claude_list_options[@]}"
	rm -f -- "$memory"
	local run=claude-user-memory
	expect_calls $run none 1 'the answer comes from what Claude loaded, not a tool'
	expect_token $run seen PROBE-USER-CLAUDE-MD-5 '#44: the user memory file is read'
	expect_token $run seen PROBE-AGENTS-MD-5 '#44: and does not stop AGENTS.md being read'
}
