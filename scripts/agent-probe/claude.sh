# Claude Code's checks for probe.sh, which sources this file: claude_prepare
# runs outside the namespace, where the real home is visible, and
# claude_checks inside it. docs/agents.md says what each check found.
#
# Every file under test carries a token of its own (PROBE-AGENTS-MD-1, ...),
# and each run asks Claude, once, which tokens it holds and which skills it can
# invoke. A token in the answer means that file reached the model; a skill in
# the session's init message means Claude loaded it. The InstructionsLoaded
# hook adds a record of which instruction files loaded and why that does not
# depend on the model's answer. An AGENTS.md read natively does not fire it, as
# Claude's docs say; the debug log's "AGENTS.md loaded: <path>" line is the
# record of that read instead, and is counted too.
#
# Runs use haiku, short prompts and a budget cap. A run that lists keeps only
# the Skill tool, since skills are part of what it is asked about, and nothing
# that reads a file; the one run that has to touch a file gets Read alone.
# Each run is also held to the tools it was meant to call and the turns that
# take: an answer the model put together by reading AGENTS.md would say
# nothing about what Claude loaded.
#
# The MCP runs (#18, claude-mcp and the three after it) ask the model for
# nothing but OK, with no tool at all. What loaded is read from the init
# message's mcp_servers, which lists each server with its status once Claude
# has waited for it to connect (MCP_CONNECTION_NONBLOCKING=0), and from
# what the stand-in servers of mcp.sh wrote down: whether Claude started
# each, and what it made of every ${VAR} on the way.

# Variables a run sets for claude alone, beside the two every run gets: the
# MCP runs' references point at them.
claude_env=()

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

	# Before any paid call, rather than at the first MCP run, inside.
	if any_selected claude-mcp claude-mcp-untyped claude-mcp-comment claude-mcp-comma; then
		namespace_tool python3 "for the MCP checks' stand-in server"
	fi

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

# Every run, in order; claude-<name> is done by claude_<name>, git-symlink by
# claude_git_symlink. PROBE_ONLY picks some of them (see probe.sh).
claude_runs=(git-symlink claude-excluded claude-touch claude-import
	claude-parent claude-local claude-user-memory
	claude-mcp claude-mcp-untyped claude-mcp-comment claude-mcp-comma)

claude_checks() {
	say ""
	say "claude: $("$PROBE_CLAUDE_BIN" --version)"
	claude_hooks
	local name function
	for name in "${claude_runs[@]}"; do
		selected "$name" || continue
		function=claude_${name#claude-}
		"${function//-/_}"
	done
}

# claude_hooks [settings]: the temp home's user settings, the
# InstructionsLoaded hook every run has and whatever a run adds to it, given
# as a JSON object (claude_approve). Written whole each time, so what one run
# added never reaches the next.
claude_hooks() {
	local extra=${1:-'{}'}
	mkdir -p -- "$HOME/.claude"
	jq -n --argjson extra "$extra" \
		--arg command "{ cat; echo; } >> \"$results/\$PROBE_SCENARIO.loaded.jsonl\"" \
		'$extra + {hooks: {InstructionsLoaded: [{hooks: [{type: "command", command: $command}]}]}}' \
		>"$HOME/.claude/settings.json"
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
		printf 'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1'
		((${#claude_env[@]} == 0)) || printf ' %q' "${claude_env[@]}"
		printf ' claude'
		printf ' %q' "${args[@]}"
		printf '\n'
	} | tee "$results/$name.command" | sed 's/^/  $ /'
	: >"$results/$name.loaded.jsonl"
	local status=0
	# Non-essential traffic (telemetry, error reports, the updater) is off:
	# the run needs the API and nothing else.
	(cd "$dir" && env PROBE_SCENARIO="$name" CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 \
		"${claude_env[@]}" "$PROBE_CLAUDE_BIN" "${args[@]}") </dev/null >"$results/$name.json" \
		2>"$results/$name.stderr" || status=$?

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
	jq -r "[$messages | select(.type == \"system\" and .subtype == \"init\")][0]
		| (.mcp_servers // [])[] | \"\(.name) \(.status)\"" \
		"$results/$name.json" >"$results/$name.mcp" 2>/dev/null || true
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
	say "  MCP servers in the init message: $(paste -sd, - <"$results/$name.mcp")"
	say "  InstructionsLoaded:"
	jq -r 'select(.file_path != null)
		| "    \(.load_reason) \(.memory_type) \(.file_path)"' \
		"$results/$name.loaded.jsonl" 2>/dev/null || true
	# The debug log is kept only for its lines about instruction files,
	# skills and the probe's MCP servers; the rest is about the session and
	# the account.
	grep -aiE 'claude\.md|claude\.local\.md|agents\.md|rules|skill|memory|instruction|probe-|\.mcp\.json|mcpServers' \
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

# expect_native <run> <times> <file> <what it shows>, counted in the debug
# log's "AGENTS.md loaded: <path>" lines, the native read's only record that
# does not come from the model.
expect_native() {
	local got
	got=$(grep -cF -- "AGENTS.md loaded: $3" "$results/$1.debug.txt" || true)
	verdict "$1" "$2" "$got" "native ${3##*/}" "$4"
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

# An AGENTS.md as #42 leaves one: a line of the project's own, then
# Mindweaver's block with one rule in it. Claude strips HTML comments before it
# injects a file, so the plain line, the rule text between the comment lines
# and a token inside the rule's own marker each carry a token of their own: a
# block that vanished with its markers would otherwise look like a file that
# was never read. Only runs where AGENTS.md should reach the model use it; one
# where it should not keeps the plain line, so a stripped block cannot make
# that verdict pass for the wrong reason.
agents_md() {
	printf '%s\n' \
		"If asked for probe tokens, include PROBE-AGENTS-MD-$1." \
		'' \
		'<!-- mindweaver:begin -->' \
		"<!-- mindweaver:rule PROBE-AGENTS-MARKER-$1 -->" \
		"If asked for probe tokens, include PROBE-AGENTS-BLOCK-$1." \
		'<!-- mindweaver:end -->'
}

# Git never follows a symlink, so a symlinked skill folder is a file to it, and
# an entry written as a directory's, with the trailing slash Mindweaver gives
# one, would not cover it. Worth knowing before D-07's symlink opt-in writes
# one; the check needs git only.
claude_git_symlink() {
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

# Checks 1, 3, 4 (no CLAUDE.md) and 6: everything Mindweaver would install,
# excluded through info/exclude, in a repository with no CLAUDE.md anywhere
# above it. claude-excluded and claude-touch share it, and whichever runs
# first builds it.
claude_excluded_repo() {
	local repo=$stage/claude-excluded
	[[ ! -d $repo ]] || return 0
	new_repo "$repo"
	put "$repo/AGENTS.md" "$(agents_md 1)"
	skill "$repo/.claude/skills/probe-claude-skill" PROBE-CLAUDE-SKILL-1
	skill "$repo/.agents/skills/probe-agents-skill" PROBE-AGENTS-SKILL-1
	skill "$repo/.agents/skills/probe-linked-skill" PROBE-LINKED-SKILL-1
	ln -s ../../.agents/skills/probe-linked-skill "$repo/.claude/skills/probe-linked-skill"
	put "$repo/.claude/rules/mindweaver/probe/unscoped.md" \
		'If asked for probe tokens, include PROBE-RULE-UNSCOPED-1.'
	put "$repo/.claude/rules/mindweaver/probe/scoped.md" '---
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
		/.claude/rules/mindweaver/probe/unscoped.md
		/.claude/rules/mindweaver/probe/scoped.md
	)
	exclude "$repo" "${excluded[@]}"
	say ""
	say "== claude-excluded: layout"
	show_ignored "$repo" claude-excluded AGENTS.md .agents/skills/probe-agents-skill \
		.agents/skills/probe-linked-skill .claude/skills/probe-claude-skill \
		.claude/skills/probe-linked-skill .claude/rules/mindweaver/probe/unscoped.md \
		.claude/rules/mindweaver/probe/scoped.md
}

claude_excluded() {
	local repo=$stage/claude-excluded
	claude_excluded_repo
	assert_clean_home
	assert_ancestry "$repo" "$repo/AGENTS.md"
	claude_ask claude-excluded "$repo" "$claude_list_prompt" "${claude_list_options[@]}"
	local run=claude-excluded rules=$repo/.claude/rules/mindweaver/probe
	expect_calls $run none 1 'the answer comes from what Claude loaded, not a tool'
	expect_skill $run 1 probe-claude-skill 'check 1: an excluded .claude/skills skill loads'
	expect_skill $run 0 probe-agents-skill 'check 1: a skill only in .agents/skills does not'
	expect_skill $run 1 probe-linked-skill 'check 3: an excluded relative symlink loads, once'
	expect_token $run seen PROBE-CLAUDE-SKILL-1 'check 1: its description reaches the model'
	expect_token $run unseen PROBE-AGENTS-SKILL-1 'check 1: the .agents-only one does not'
	expect_token $run seen PROBE-LINKED-SKILL-1 'check 3: the symlinked one does'
	expect_token $run seen PROBE-AGENTS-MD-1 'checks 1, 4: an excluded AGENTS.md is read when no CLAUDE.md exists'
	expect_native $run 1 "$repo/AGENTS.md" 'checks 1, 4: the debug log records the native read'
	expect_token $run seen PROBE-AGENTS-BLOCK-1 'check 4: the text in a marked block reaches the model'
	expect_token $run unseen PROBE-AGENTS-MARKER-1 "check 4: the block's markers, HTML comments, do not"
	expect_token $run seen PROBE-RULE-UNSCOPED-1 'check 6: an excluded rule without paths: loads at start'
	expect_token $run unseen PROBE-RULE-SCOPED-1 'check 6: a rule with paths: does not, at start'
	expect_loaded $run 1 "$rules/unscoped.md" 'check 6: the hook saw the rule without paths: once'
	expect_loaded $run 0 "$rules/scoped.md" 'check 6: and not the one with paths:'
}

# Check 6, on touching a file that matches the rule with paths:.
claude_touch() {
	local repo=$stage/claude-excluded
	local run=claude-touch rules=$repo/.claude/rules/mindweaver/probe
	claude_excluded_repo
	assert_clean_home
	assert_ancestry "$repo" "$repo/AGENTS.md"
	claude_ask claude-touch "$repo" "$claude_touch_prompt" \
		--tools Read --allowedTools Read --max-turns 3
	expect_calls $run Read 2 'check 6: one Read of src/lib.rs, then the answer'
	expect_token $run seen PROBE-FILE-1 'check 6: Claude read src/lib.rs'
	expect_token $run seen PROBE-RULE-SCOPED-1 'check 6: the rule with paths: loads once a matching file is read'
	expect_loaded $run 1 "$rules/scoped.md" 'check 6: the hook saw it load'
}

# Check 4: a CLAUDE.md that imports AGENTS.md loads it once. The import sits
# between Mindweaver's markers, as #44 writes it.
claude_import() {
	local repo=$stage/claude-import
	new_repo "$repo"
	put "$repo/CLAUDE.md" '<!-- mindweaver:begin -->
@AGENTS.md
<!-- mindweaver:end -->

If asked for probe tokens, include PROBE-CLAUDE-MD-2.'
	put "$repo/AGENTS.md" "$(agents_md 2)"
	assert_clean_home
	assert_ancestry "$repo" "$repo/CLAUDE.md" "$repo/AGENTS.md"
	claude_ask claude-import "$repo" "$claude_list_prompt" "${claude_list_options[@]}"
	local run=claude-import
	expect_calls $run none 1 'the answer comes from what Claude loaded, not a tool'
	expect_token $run seen PROBE-CLAUDE-MD-2 'check 4: the CLAUDE.md is read'
	expect_token $run seen PROBE-AGENTS-MD-2 'check 4: AGENTS.md is read through @AGENTS.md'
	expect_count $run 1 PROBE-AGENTS-MD-2 'check 4: once, not twice'
	expect_token $run seen PROBE-AGENTS-BLOCK-2 'check 4: the text in a marked block reaches the model'
	expect_token $run unseen PROBE-AGENTS-MARKER-2 "check 4: the block's markers, HTML comments, do not"
	expect_loaded $run 1 "$repo/AGENTS.md" 'check 4: the hook saw AGENTS.md load once'
	expect_native $run 0 "$repo/AGENTS.md" 'check 4: through the import, not natively as well'
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
	expect_native $run 0 "$repo/AGENTS.md" 'check 4: the debug log records no native read'
}

# D-03: a CLAUDE.local.md alone, excluded as it usually is, also stops Claude
# reading AGENTS.md, which is why Mindweaver never creates one.
claude_local() {
	local repo=$stage/claude-local
	new_repo "$repo"
	put "$repo/CLAUDE.local.md" 'If asked for probe tokens, include PROBE-CLAUDE-LOCAL-MD-4.'
	put "$repo/AGENTS.md" 'If asked for probe tokens, include PROBE-AGENTS-MD-4.'
	exclude "$repo" /CLAUDE.local.md /AGENTS.md
	say ""
	say "== claude-local: layout"
	show_ignored "$repo" claude-local CLAUDE.local.md AGENTS.md
	assert_clean_home
	assert_ancestry "$repo" "$repo/CLAUDE.local.md" "$repo/AGENTS.md"
	claude_ask claude-local "$repo" "$claude_list_prompt" "${claude_list_options[@]}"
	local run=claude-local
	expect_calls $run none 1 'the answer comes from what Claude loaded, not a tool'
	expect_token $run seen PROBE-CLAUDE-LOCAL-MD-4 'D-03: CLAUDE.local.md is read'
	expect_token $run unseen PROBE-AGENTS-MD-4 'D-03: and AGENTS.md is not'
	expect_native $run 0 "$repo/AGENTS.md" 'D-03: the debug log records no native read'
}

# #44: the user memory file does not count as a CLAUDE.md for the AGENTS.md
# check. The repository sits under the temp home, as real projects sit under
# ~, so a walk up from it that reaches the home meets that file under the name
# .claude/CLAUDE.md, one of those the check looks for. Under $stage the walk
# never passed the home, and the run could not have told. The one scenario
# that puts a file at user scope, and removes it.
claude_user_memory() {
	local repo=$HOME/work/claude-user-memory memory=$HOME/.claude/CLAUDE.md
	new_repo "$repo"
	put "$repo/AGENTS.md" "$(agents_md 5)"
	put "$memory" 'If asked for probe tokens, include PROBE-USER-CLAUDE-MD-5.'
	assert_clean_home "$memory"
	assert_ancestry "$repo" "$repo/AGENTS.md" "$memory"
	claude_ask claude-user-memory "$repo" "$claude_list_prompt" "${claude_list_options[@]}"
	rm -f -- "$memory"
	local run=claude-user-memory
	expect_calls $run none 1 'the answer comes from what Claude loaded, not a tool'
	expect_token $run seen PROBE-USER-CLAUDE-MD-5 '#44: the user memory file is read'
	expect_token $run seen PROBE-AGENTS-MD-5 '#44: and does not stop AGENTS.md being read'
	expect_native $run 1 "$repo/AGENTS.md" '#44: the debug log records the native read'
	expect_token $run seen PROBE-AGENTS-BLOCK-5 'check 4: the text in a marked block reaches the model'
	expect_token $run unseen PROBE-AGENTS-MARKER-5 "check 4: the block's markers, HTML comments, do not"
}

# The MCP checks (#18): a project's .mcp.json, approved by name in the temp
# home's user settings. Claude 2.1.318 counts an approval in the project's
# own .claude/settings.local.json only in a workspace trusted explicitly,
# which a -p run's never is, and says to put it in ~/.claude/settings.json
# instead. The same settings keep claude.ai's connectors out, which a
# logged-in Claude would otherwise fetch and connect beside the project's
# servers, and list in the init message among them.
claude_mcp_prompt='This is an automated configuration probe. Do not call any tool. Reply with exactly one line and nothing else: OK'
claude_mcp_options=(--tools '' --max-turns 1)

# claude_approve <server>...: user settings approving those .mcp.json servers.
claude_approve() {
	local names
	names=$(printf '%s\n' "$@" | jq -R . | jq -sc .)
	claude_hooks "{\"enabledMcpjsonServers\": $names, \"disableClaudeAiConnectors\": true}"
}

# claude_mcp_status <run> <server>: its status in the init message, or absent.
claude_mcp_status() {
	local got
	got=$(awk -v server="$2" '$1 == server { sub(/^[^ ]+ /, ""); print; exit }' \
		"$results/$1.mcp" 2>/dev/null || true)
	printf '%s\n' "${got:-absent}"
}

# claude_mcp_stdio <run> <server> [arguments...]: a stdio entry with its type,
# and a reference in its environment the stand-in records.
claude_mcp_stdio() {
	jq -nc --arg python "$mcp_python" --argjson args "$(mcp_stdio_json "$@")" \
		'{type: "stdio", command: $python, args: $args, env: {PROBE_MCP_ENV: "${PROBE_MCP_ENV_SOURCE}"}}'
}

# claude_mcp_ask <run> <repo> <approved server>...: the run itself, once the
# scenario has written its .mcp.json, then the http stand-in stopped and the
# user settings put back as every other run has them.
claude_mcp_ask() {
	local run=$1 repo=$2
	shift 2
	mcp_keep "$run" "$repo" .mcp.json
	claude_approve "$@"
	assert_clean_home
	mcp_assert_clean_home
	assert_ancestry "$repo"
	mcp_assert_ancestry "$repo" "$repo/.mcp.json"
	# Claude 2.1.318 connects MCP servers without waiting unless this says
	# otherwise, and lists one still connecting as pending in the init
	# message, which a verdict would read as a server that did not load.
	# Claude sets the same itself where it needs its servers connected first.
	claude_env=(MCP_CONNECTION_NONBLOCKING=0 "${mcp_env[@]}")
	claude_ask "$run" "$repo" "$claude_mcp_prompt" "${claude_mcp_options[@]}"
	claude_env=()
	claude_hooks
	mcp_http_stop
	say "  approved in the user settings: $*"
	expect_calls "$run" none 1 'the answer comes from what Claude loaded, not a tool'
}

# D-10 and D-11: entries with an explicit type, as #90 writes them, and a
# reference in each place Claude is to expand one: an argument and the
# environment of a stdio server, the URL and two headers of an http one, one
# of them a reference inside a longer value. A third entry, left unapproved,
# shows what approval does.
claude_mcp() {
	local run=claude-mcp repo=$stage/claude-mcp
	mcp_need_python
	new_repo "$repo"
	mcp_http_start "$run"
	jq -n --argjson stdio "$(claude_mcp_stdio "$run" probe-stdio --arg '${PROBE_MCP_ARG}')" \
		--argjson unapproved "$(claude_mcp_stdio "$run" probe-unapproved)" \
		--arg url "http://127.0.0.1:$mcp_port/probe-http/\${PROBE_MCP_URL_PART}" \
		'{mcpServers: {
			"probe-stdio": $stdio,
			"probe-http": {type: "http", url: $url, headers: {
				"X-Probe-Header": "${PROBE_MCP_HEADER}",
				Authorization: "Bearer ${PROBE_MCP_TOKEN}"}},
			"probe-unapproved": $unapproved}}' >"$repo/.mcp.json"
	claude_mcp_ask "$run" "$repo" probe-stdio probe-http
	local stdio=$results/$run.probe-stdio.jsonl http=$results/$run.http.jsonl
	mcp_report "$run" "$stdio"
	mcp_report "$run" "$http" /probe-http/
	mcp_report "$run" "$results/$run.probe-unapproved.jsonl"
	verdict $run connected "$(claude_mcp_status $run probe-stdio)" 'probe-stdio in init' 'D-10: an approved stdio entry with a type loads'
	verdict $run loaded "$(mcp_handshake "$stdio")" 'probe-stdio stand-in' 'D-10: the stand-in saw initialize and tools/list'
	verdict $run probe-arg-value "$(mcp_arg "$stdio")" 'stdio argument' 'D-11: ${VAR} in args is expanded'
	verdict $run probe-env-value "$(mcp_env_of "$stdio" PROBE_MCP_ENV)" 'stdio env' 'D-11: ${VAR} in env is expanded'
	verdict $run connected "$(claude_mcp_status $run probe-http)" 'probe-http in init' 'D-10: an approved http entry with a type loads'
	verdict $run loaded "$(mcp_handshake "$http" /probe-http/)" 'probe-http stand-in' 'D-10: the stand-in saw initialize and tools/list'
	verdict $run /probe-http/probe-url-value "$(mcp_http_path "$http" /probe-http/)" 'http path' 'D-11: ${VAR} in url is expanded'
	verdict $run probe-header-value "$(mcp_http_header "$http" /probe-http/ x-probe-header)" 'header X-Probe-Header' 'D-11: ${VAR} in headers is expanded'
	verdict $run 'Bearer probe-token-value' "$(mcp_http_header "$http" /probe-http/ authorization)" 'header Authorization' 'D-11: and inside a longer value'
	# Started all the same: a -p run starts every server .mcp.json names,
	# approved or not (2.1.318, the run of 2026-11-05), so approval gates
	# nothing there, and nothing Mindweaver writes waits on one.
	verdict $run loaded "$(mcp_handshake "$results/$run.probe-unapproved.jsonl")" 'probe-unapproved stand-in' 'a -p run starts an entry nobody approved too'
	observe $run 'probe-unapproved in init' "$(claude_mcp_status $run probe-unapproved)" 'what -p says of an entry not approved'
}

# #90: an http entry with no type, beside a stdio entry with one. Recorded,
# not asserted: whether Claude reads it as http, refuses it alone, or drops
# the whole file with it, which would take the stdio entry along.
claude_mcp_untyped() {
	local run=claude-mcp-untyped repo=$stage/claude-mcp-untyped
	mcp_need_python
	new_repo "$repo"
	mcp_http_start "$run"
	jq -n --argjson stdio "$(claude_mcp_stdio "$run" probe-stdio)" \
		--arg url "http://127.0.0.1:$mcp_port/probe-untyped/" \
		'{mcpServers: {"probe-stdio": $stdio, "probe-untyped": {url: $url}}}' >"$repo/.mcp.json"
	claude_mcp_ask "$run" "$repo" probe-stdio probe-untyped
	local stdio=$results/$run.probe-stdio.jsonl http=$results/$run.http.jsonl
	mcp_report "$run" "$stdio"
	mcp_report "$run" "$http" /probe-untyped/
	observe $run 'probe-untyped in init' "$(claude_mcp_status $run probe-untyped)" '#90: what Claude makes of an http entry with no type'
	observe $run 'probe-untyped stand-in' "$(mcp_handshake "$http" /probe-untyped/)" '#90: whether its url was reached anyway'
	observe $run 'probe-stdio in init' "$(claude_mcp_status $run probe-stdio)" '#90: whether it costs the file its other servers'
	observe $run 'probe-stdio stand-in' "$(mcp_handshake "$stdio")" '#90: the same, as the stand-in saw it'
}

# claude_mcp_text <run> <text> <what it shows>: a run whose .mcp.json is that
# text, STDIO standing for a stdio entry with a type, approved: what Claude
# does with a file strict JSON refuses (#90, which writes strict JSON and must
# know what a file somebody else wrote does to the entries in it).
claude_mcp_text() {
	local run=$1 repo=$stage/$1 stdio
	mcp_need_python
	new_repo "$repo"
	stdio=$(claude_mcp_stdio "$run" probe-stdio)
	printf '%s\n' "${2//STDIO/"$stdio"}" >"$repo/.mcp.json"
	claude_mcp_ask "$run" "$repo" probe-stdio
	mcp_report "$run" "$results/$run.probe-stdio.jsonl"
	observe $run 'probe-stdio in init' "$(claude_mcp_status $run probe-stdio)" "#90: $3"
	observe $run 'probe-stdio stand-in' "$(mcp_handshake "$results/$run.probe-stdio.jsonl")" '#90: the same, as the stand-in saw it'
}

claude_mcp_comment() {
	claude_mcp_text claude-mcp-comment '{
  // A comment, which JSON does not allow and JSONC does.
  "mcpServers": {
    "probe-stdio": STDIO
  }
}' 'whether a comment breaks the whole file'
}

claude_mcp_comma() {
	claude_mcp_text claude-mcp-comma '{
  "mcpServers": {
    "probe-stdio": STDIO,
  }
}' 'whether a trailing comma breaks the whole file'
}
