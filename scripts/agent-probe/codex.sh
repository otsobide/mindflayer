# Codex's checks for probe.sh, which sources this file: codex_prepare runs
# outside the namespace, where the real home is visible, and codex_checks
# inside it. docs/agents.md says what each check found.
#
# Only the MCP checks of #18 so far. Where Codex reads skills and rules stays
# docs-only in docs/agents.md, as #17 left it, until runs about them join
# codex_runs here.
#
# Each run gives a project a .codex/config.toml holding one stdio server and
# one http server, and writes the temp home's own config.toml whole, trust
# entry and all, so nothing of one run reaches the next. What loaded is read
# from what the stand-ins of mcp.sh saw. Beside it each run keeps what
# `codex mcp list --json` says the configuration holds, which calls no model,
# and makes one `codex exec`: Codex starts a session's MCP servers only for a
# session. That one asks for OK alone, on a small model at low effort, with
# the sandbox read-only, and is held to calling nothing.

codex_runs=(codex-mcp-trusted codex-mcp-untrusted codex-mcp-collision)

# The smallest model the 0.166 binary's own catalog lists, "fast and
# affordable model for easier tasks". Whether the operator's account offers
# it is for the run to show; PROBE_CODEX_MODEL names another.
codex_default_model=gpt-6-luna

codex_prompt='This is an automated configuration probe. Do not call any tool and do not run any command. Reply with exactly one line and nothing else: OK'

codex_prepare() {
	local bin=${PROBE_CODEX_BIN:-}
	if [[ -z $bin ]]; then
		bin=$(command -v codex) || die "no codex on PATH (or set PROBE_CODEX_BIN)"
	fi
	bin=$(readlink -f -- "$bin")
	# A launcher script, as an npm install gives, would run node and a
	# package under the real home, which the namespace hides.
	[[ $(head -c 2 -- "$bin") != '#!' ]] ||
		die "$bin is a launcher script: set PROBE_CODEX_BIN to the native codex binary its package ships"
	# Asking a stand-in what it is would run it out here, against the real
	# home, so it is told by its name and its first bytes: a version
	# manager's shim resolves to the manager itself. The version is asked
	# inside, where the home is the temp one.
	[[ ${bin##*/} == codex && $(head -c 4 -- "$bin") == $'\x7fELF' ]] ||
		die "$bin is not the codex binary: set PROBE_CODEX_BIN to it, as \`mise which codex\` prints it"
	# A release's binary keeps its helpers beside it (codex-resources,
	# codex-path), so the whole package goes into the namespace.
	local package
	package=$(dirname "$(dirname "$bin")")
	if [[ -f $package/codex-package.json ]]; then
		expose+=("$package")
	else
		expose+=("$bin")
	fi

	# Only the login, never the rest of the folder: the operator's config,
	# trust entries and servers are exactly what must not take part.
	local login=${CODEX_HOME:-$real_home/.codex}/auth.json
	[[ -f $login ]] ||
		die "no Codex login at $login: run codex login first, with the login kept in that file rather than the keyring"
	# A ChatGPT login is refreshed by whichever Codex finds it due, and a
	# refresh can rotate the refresh token, leaving the copy in the real home
	# holding one that no longer works. So a login close to that is refused,
	# as Claude's is: an access token with less than 15 minutes left, or a
	# last refresh more than seven days ago. Codex has refreshed past eight;
	# that 0.166 still does is an assumption the run cannot confirm.
	local mode expires refreshed left=unknown age=unknown
	mode=$(jq -r '.auth_mode // (if .tokens then "chatgpt" elif .OPENAI_API_KEY then "apikey" else "unknown" end)' "$login")
	expires=$(jq -r '.tokens.access_token // empty | split(".")[1] // empty
		| gsub("-"; "+") | gsub("_"; "/") | . + ("=" * ((4 - length % 4) % 4))
		| @base64d | fromjson | .exp // empty' "$login" 2>/dev/null) || expires=
	if [[ $expires =~ ^[0-9]+$ ]]; then
		left=$(((expires - $(date +%s)) / 60))
		((left >= 15)) || die "the Codex login's access token expires in $left minutes: refresh it (run codex once) and try again"
	fi
	refreshed=$(jq -r '.last_refresh // empty' "$login")
	if [[ -n $refreshed ]] && refreshed=$(date -d "$refreshed" +%s 2>/dev/null); then
		age=$((($(date +%s) - refreshed) / 86400))
		((age < 7)) || die "the Codex login was last refreshed $age days ago: refresh it (run codex once) and try again"
	fi
	install -d -m 700 -- "$run/home/.codex"
	install -m 600 -- "$login" "$run/home/.codex/auth.json"
	codex_login_hash=$(sha256sum <"$run/home/.codex/auth.json")

	namespace_env+=(PROBE_CODEX_BIN "$bin" PROBE_CODEX_MODEL "${PROBE_CODEX_MODEL:-$codex_default_model}")
	say "codex: $bin, its version asked inside"
	say "codex: login ($mode) copied from $login, access token valid for another $left minutes, refreshed $age days ago"
}

codex_finish() {
	[[ -f $run/home/.codex/auth.json ]] || return 0
	[[ $(sha256sum <"$run/home/.codex/auth.json") == "$codex_login_hash" ]] && return 0
	say "WARNING: Codex refreshed its login during the run; if the real session"
	say "         stops authenticating, log in again with codex login."
}

codex_checks() {
	say ""
	local version
	version=$(timeout 60 "$PROBE_CODEX_BIN" --version 2>/dev/null | head -n 1) || version=
	[[ $version == codex* ]] ||
		die "PROBE_CODEX_BIN answers '${version:-nothing}' to --version, not as Codex does"
	say "codex: $version, model $PROBE_CODEX_MODEL"
	local name function
	for name in "${codex_runs[@]}"; do
		selected "$name" || continue
		function=codex_${name#codex-}
		"${function//-/_}"
	done
}

# codex_user_config <repo> <trusted|untrusted|none> [TOML]: the temp home's
# config.toml for one run: the login read from its file, the project marked
# as the run says, and whatever else the run adds.
codex_user_config() {
	{
		printf '# Written by probe.sh for one run (#18).\n'
		printf 'cli_auth_credentials_store = "file"\n'
		[[ $2 == none ]] || printf '\n[projects."%s"]\ntrust_level = "%s"\n' "$1" "$2"
		[[ -z ${3:-} ]] || printf '\n%s\n' "$3"
	} >"$CODEX_HOME/config.toml"
}

# codex_scenario <run> <repo>: a fresh repository whose .codex/config.toml
# holds the two servers every run has, and the http stand-in they reach. The
# references are D-11's question: Codex has no interpolation, so ${VAR} in
# args, env and url should reach the server as written, while env_vars,
# env_http_headers and bearer_token_env_var name the variables to fill a
# value from.
codex_scenario() {
	local run=$1 repo=$2
	mcp_need_python
	new_repo "$repo"
	mcp_http_start "$run"
	mkdir -p -- "$repo/.codex"
	cat >"$repo/.codex/config.toml" <<EOF
[mcp_servers.probe-stdio]
command = "$mcp_python"
args = $(mcp_stdio_json "$run" probe-stdio --arg '${PROBE_MCP_ARG}')
env = { PROBE_MCP_ENV = "\${PROBE_MCP_ENV_SOURCE}" }
env_vars = ["PROBE_MCP_ENV_SOURCE"]

[mcp_servers.probe-http]
url = "http://127.0.0.1:$mcp_port/probe-http/\${PROBE_MCP_URL_PART}"
env_http_headers = { "X-Probe-Header" = "PROBE_MCP_HEADER" }
bearer_token_env_var = "PROBE_MCP_TOKEN"
EOF
}

# codex_list <name> <dir>: what `codex mcp list --json` says from there.
codex_list() {
	local name=$1 dir=$2 status=0
	(cd "$dir" && env "${mcp_env[@]}" timeout --kill-after=10 60 "$PROBE_CODEX_BIN" mcp list --json) \
		</dev/null >"$results/$name.list.json" 2>"$results/$name.list.stderr" || status=$?
	printf '%s\n' "$status" >"$results/$name.list.status"
	say "  \$ codex mcp list --json: exit $status, lists $(codex_listed "$name")"
	if [[ $status != 0 ]]; then
		head -n 5 -- "$results/$name.list.stderr" | sed 's/^/    /'
	fi
}

# codex_listed <name>: the servers that list named, or how it failed.
codex_listed() {
	local names status
	status=$(<"$results/$1.list.status")
	names=$(jq -r '(if type == "array" then . else (.servers // .mcp_servers // []) end)[]?
		| .name // empty' "$results/$1.list.json" 2>/dev/null | paste -sd, -) || names=
	if [[ -n $names ]]; then
		printf '%s\n' "$names"
	elif [[ $status != 0 ]]; then
		printf 'error (exit %s)\n' "$status"
	else
		printf 'none\n'
	fi
}

# codex_exec <name> <dir>: the run's one session, which sets codex_status,
# codex_reached (whether the turn reached the model) and codex_tools (any
# item but a message or reasoning: a command, a tool call, an edit).
codex_exec() {
	local name=$1 dir=$2
	# --skip-git-repo-check: what an untrusted project does to the servers
	# is the question, and exec's own refusal of a folder it does not trust
	# would answer it for the wrong reason.
	local args=(exec --json --skip-git-repo-check --sandbox read-only
		--model "$PROBE_CODEX_MODEL" -c 'model_reasoning_effort="low"' "$codex_prompt")
	say ""
	say "== $name"
	{
		printf 'cd %q\n' "$dir"
		printf '%s ' "${mcp_env[@]}"
		printf 'codex'
		printf ' %q' "${args[@]}"
		printf '\n'
	} | tee "$results/$name.command" | sed 's/^/  $ /'
	# The session files are the temp home's; each run reads only its own.
	rm -rf -- "$CODEX_HOME/sessions"
	codex_status=0
	(cd "$dir" && env "${mcp_env[@]}" timeout --kill-after=10 300 "$PROBE_CODEX_BIN" "${args[@]}") \
		</dev/null >"$results/$name.jsonl" 2>"$results/$name.stderr" || codex_status=$?
	grep -rh -- '"mcp_startup' "$CODEX_HOME/sessions" >"$results/$name.rollout-mcp.jsonl" 2>/dev/null || true
	local events='fromjson? | select(type == "object")' reached answer usage
	reached=$(jq -Rr "$events | select(.type == \"turn.completed\"
		or (.type == \"item.completed\" and .item.type == \"agent_message\")) | .type" \
		"$results/$name.jsonl" 2>/dev/null) || reached=
	codex_reached=no
	[[ -z $reached ]] || codex_reached=yes
	codex_tools=$(jq -Rr "$events | select(.type == \"item.started\" or .type == \"item.completed\")
		| .item.type // empty | select(. != \"agent_message\" and . != \"reasoning\")" \
		"$results/$name.jsonl" 2>/dev/null | sort -u | paste -sd, -) || codex_tools=
	codex_tools=${codex_tools:-none}
	answer=$(jq -Rr "$events | select(.type == \"item.completed\" and .item.type == \"agent_message\")
		| .item.text" "$results/$name.jsonl" 2>/dev/null | tail -n 1) || answer=
	usage=$(jq -Rc "$events | select(.type == \"turn.completed\") | .usage" \
		"$results/$name.jsonl" 2>/dev/null | tail -n 1) || usage=
	say "  exit $codex_status, reached the model: $codex_reached, usage ${usage:-none}"
	say "  answer: ${answer:-none}"
	say "  items besides messages: $codex_tools"
	if [[ $codex_status != 0 ]]; then
		say "  stderr:"
		head -n 20 -- "$results/$name.stderr" | sed 's/^/    /'
	fi
	if [[ -s $results/$name.rollout-mcp.jsonl ]]; then
		say "  MCP startup, as the session file has it:"
		sed 's/^/    /' "$results/$name.rollout-mcp.jsonl"
	fi
}

# codex_guard <repo>: nothing above the scenario or at user scope but what
# the run wrote, before Codex is asked anything there.
codex_guard() {
	assert_clean_home
	mcp_assert_clean_home
	assert_ancestry "$1"
	mcp_assert_ancestry "$1" "$1/.codex/config.toml"
}

# codex_run <run> <repo>: the list and the session, once the run has written
# both config files, then the http stand-in stopped.
codex_run() {
	local run=$1 repo=$2
	mcp_keep "$run" "$repo" .codex/config.toml
	cp -- "$CODEX_HOME/config.toml" "$results/$run.given.home.config.toml"
	codex_guard "$repo"
	codex_list "$run" "$repo"
	codex_exec "$run" "$repo"
	mcp_http_stop
	mcp_report "$run" "$results/$run.probe-stdio.jsonl"
	mcp_report "$run" "$results/$run.http.jsonl" /probe-http/
}

# D-10 and D-11 in a trusted project: both servers load, ${VAR} reaches each
# server as written, and the three keys that name a variable fill a value.
codex_mcp_trusted() {
	local run=codex-mcp-trusted repo=$stage/codex-mcp-trusted
	codex_scenario "$run" "$repo"
	codex_user_config "$repo" trusted
	codex_run "$run" "$repo"
	local stdio=$results/$run.probe-stdio.jsonl http=$results/$run.http.jsonl
	verdict $run yes "$codex_reached" 'reached the model' 'the session ran, so its servers were started'
	verdict $run none "$codex_tools" 'items besides messages' 'the answer comes from what Codex loaded, not a tool'
	verdict $run loaded "$(mcp_handshake "$stdio")" 'probe-stdio stand-in' "D-10: a trusted project's stdio entry loads"
	verdict $run '${PROBE_MCP_ARG}' "$(mcp_arg "$stdio")" 'stdio argument' 'D-11: ${VAR} in args reaches the server as written'
	verdict $run '${PROBE_MCP_ENV_SOURCE}' "$(mcp_env_of "$stdio" PROBE_MCP_ENV)" 'stdio env' 'D-11: and in env'
	verdict $run probe-env-value "$(mcp_env_of "$stdio" PROBE_MCP_ENV_SOURCE)" 'stdio env_vars' 'D-11: env_vars passes a variable on by name'
	observe $run 'stdio env, not named' "$(mcp_env_of "$stdio" PROBE_MCP_HEADER)" "#91: whether Codex passes a server the variables it names only"
	verdict $run loaded "$(mcp_handshake "$http" /probe-http/)" 'probe-http stand-in' "D-10: a trusted project's http entry loads"
	verdict $run '/probe-http/${PROBE_MCP_URL_PART}' "$(mcp_http_path "$http" /probe-http/)" 'http path' 'D-11: ${VAR} in url reaches the server as written'
	verdict $run probe-header-value "$(mcp_http_header "$http" /probe-http/ x-probe-header)" 'header X-Probe-Header' 'D-11: env_http_headers fills a header from a variable'
	verdict $run 'Bearer probe-token-value' "$(mcp_http_header "$http" /probe-http/ authorization)" 'header Authorization' 'D-11: bearer_token_env_var fills Authorization'
	observe $run 'mcp list' "$(codex_listed $run)" '#96: what the configuration holds, read without a session'
}

# D-10 in an untrusted project: the same .codex/config.toml is not read. The
# list is asked first with no trust entry at all, which is what #91 warns of.
codex_mcp_untrusted() {
	local run=codex-mcp-untrusted repo=$stage/codex-mcp-untrusted
	codex_scenario "$run" "$repo"
	codex_user_config "$repo" none
	codex_guard "$repo"
	codex_list "$run.unmarked" "$repo"
	codex_user_config "$repo" untrusted
	codex_run "$run" "$repo"
	verdict $run yes "$codex_reached" 'reached the model' 'the session ran, so it would have started a server it read'
	verdict $run none "$codex_tools" 'items besides messages' 'the answer comes from what Codex loaded, not a tool'
	verdict $run none "$(mcp_handshake "$results/$run.probe-stdio.jsonl")" 'probe-stdio stand-in' "D-10: an untrusted project's stdio entry is not started"
	verdict $run none "$(mcp_handshake "$results/$run.http.jsonl" /probe-http/)" 'probe-http stand-in' "D-10: nor its http entry"
	observe $run 'mcp list, untrusted' "$(codex_listed $run)" '#96: what the list says of an untrusted project'
	observe $run 'mcp list, no entry' "$(codex_listed $run.unmarked)" '#91: what it says of a project with no trust entry'
}

# #91: the user's own config.toml names the project's stdio server too, as an
# http one. Codex merges layers key by key, so the two may make one entry with
# both a command and a url. Recorded, not asserted: whether that breaks the
# whole configuration, every other server and the session with it, which has
# #91 refuse such a name, or one layer wins, which has it warn.
codex_mcp_collision() {
	local run=codex-mcp-collision repo=$stage/codex-mcp-collision
	codex_scenario "$run" "$repo"
	codex_user_config "$repo" trusted "[mcp_servers.probe-stdio]
url = \"http://127.0.0.1:$mcp_port/probe-stdio-user/\""
	codex_run "$run" "$repo"
	local stdio http user list_status outcome
	stdio=$(mcp_handshake "$results/$run.probe-stdio.jsonl")
	http=$(mcp_handshake "$results/$run.http.jsonl" /probe-http/)
	user=$(mcp_handshake "$results/$run.http.jsonl" /probe-stdio-user/)
	list_status=$(<"$results/$run.list.status")
	if [[ $codex_reached == no && $http == none ]]; then
		if [[ $list_status != 0 ]]; then
			outcome='whole config refused'
		else
			outcome='session refused'
		fi
	elif [[ $http == none ]]; then
		outcome='other servers lost'
	elif [[ $stdio != none && $user == none ]]; then
		outcome='project layer wins'
	elif [[ $stdio == none && $user != none ]]; then
		outcome='user layer wins'
	elif [[ $stdio == none && $user == none ]]; then
		outcome='that name dropped'
	else
		outcome='both started'
	fi
	observe $run 'outcome' "$outcome" '#91: refuse the name if the whole config is refused, else warn'
	observe $run 'mcp list' "$(codex_listed $run)" '#91: what the list makes of the two layers'
	observe $run 'exec exit' "$codex_status" '#91: whether the session starts at all'
	observe $run 'reached the model' "$codex_reached" '#91: the same, past the configuration'
	observe $run 'probe-stdio, project stdio' "$stdio" '#91: the project layer'
	observe $run 'probe-stdio, user http' "$user" '#91: the user layer'
	observe $run 'probe-http, no collision' "$http" '#91: whether a collision costs the other servers'
}
