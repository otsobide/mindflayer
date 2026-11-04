# The MCP checks' shared parts (#18), which probe.sh sources inside the
# namespace before any agent's checks: the stand-in server each run starts,
# the variables its references name, and the readers of what the server
# wrote down. docs/agents.md says what each check found.
#
# A run gives the agent a stdio server and an http server, both instances of
# mcp_server.py, and puts a reference to a variable in each place the run is
# about: an argument, the server's environment, the URL, a header. Each
# variable holds a value of its own, so what the server was started with, or
# was sent, says whether the agent expanded the reference, passed it through
# as written or dropped it, and whether it started the server at all, with
# nothing asked of the model.

mcp_script=$mount_scripts/mcp_server.py

# What every run's references name, set for the agent alone. Each value says
# "probe", which is what the server keeps of a value (see mcp_server.py).
mcp_env=(
	PROBE_MCP_ARG=probe-arg-value
	PROBE_MCP_ENV_SOURCE=probe-env-value
	PROBE_MCP_URL_PART=probe-url-value
	PROBE_MCP_HEADER=probe-header-value
	PROBE_MCP_TOKEN=probe-token-value
)

# The stand-in runs under the agent, inside the namespace, where PATH is
# /usr/bin:/bin. One found only in a version manager's folder would pass out
# there and be missing in here, where every server would then fail to start
# and read as one the agent never loaded.
mcp_need_python() {
	local candidate
	for candidate in /usr/bin/python3 /bin/python3; do
		[[ -x $candidate ]] || continue
		"$candidate" -I -c 'import sys; sys.exit(sys.version_info < (3, 7))' ||
			die "$candidate is older than 3.7, which the MCP stand-in server needs"
		mcp_python=$candidate
		return 0
	done
	die "the MCP checks need python3 in /usr/bin or /bin, the only PATH inside the namespace, to run their stand-in server"
}

# mcp_http_start <run>: the run's http server, on a port the kernel picks,
# which each of the run's http entries reaches under a path of its own. Its
# log is the run's: <run>.http.jsonl.
mcp_http_start() {
	local run=$1 port_file=$results/$1.http.port i
	rm -f -- "$port_file"
	# Nothing of the probe's own output is handed to it: a server left
	# running would hold a pipe the operator reads the probe through open
	# after the probe is done.
	"$mcp_python" -I "$mcp_script" http --name "$run" --log "$results/$run.http.jsonl" \
		--port-file "$port_file" </dev/null >/dev/null 2>"$results/$run.http.stderr" &
	mcp_http_pid=$!
	for ((i = 0; i < 50; i++)); do
		[[ -s $port_file ]] && break
		kill -0 "$mcp_http_pid" 2>/dev/null ||
			die "the http stand-in for $run did not start: $(head -c 500 -- "$results/$run.http.stderr")"
		sleep 0.1
	done
	[[ -s $port_file ]] || die "the http stand-in for $run gave no port within 5 seconds"
	mcp_port=$(<"$port_file")
}

mcp_http_stop() {
	[[ -n ${mcp_http_pid:-} ]] || return 0
	kill -- "$mcp_http_pid" 2>/dev/null || true
	wait -- "$mcp_http_pid" 2>/dev/null || true
	mcp_http_pid=
}

# mcp_stdio_json <run> <server> [more arguments...]: the arguments that start
# the stdio stand-in under that name, as a JSON array, which is also a TOML
# one. Its log is <run>.<server>.jsonl.
mcp_stdio_json() {
	{
		printf '%s\n' -I "$mcp_script" stdio --name "$2" --log "$results/$1.$2.jsonl"
		# printf given no word still prints its format once: one empty word.
		if (($# > 2)); then printf '%s\n' "${@:3}"; fi
	} | jq -R . | jq -sc .
}

# mcp_keep <run> <repo> <file>...: each config file the run wrote, kept beside
# its results as <run>.given.<route>, the route's slashes as dots and its
# leading dot dropped: claude-mcp.given.mcp.json.
mcp_keep() {
	local run=$1 repo=$2 file name
	shift 2
	for file in "$@"; do
		name=${file#.}
		cp -- "$repo/$file" "$results/$run.given.${name//\//.}"
	done
}

# Nothing at user scope may hold an MCP server: one there is loaded beside
# the project's, whichever the run is about, and could answer for it.
# Claude keeps a user's and a local scope's servers in its .claude.json,
# which an earlier run in the temp home will have made.
mcp_assert_clean_home() {
	local path count
	for path in "$HOME/.mcp.json" "$HOME/.config/opencode/opencode.json" \
		"$HOME/.config/opencode/opencode.jsonc" "$HOME/.config/opencode/config.json"; do
		[[ ! -e $path ]] || die "user scope in the temp home holds MCP configuration: $path"
	done
	for path in "$HOME/.claude/.claude.json" "$HOME/.claude.json"; do
		[[ -f $path ]] || continue
		count=$(jq '[.mcpServers // {}, (.projects // {} | .[] | .mcpServers // {})]
			| map(length) | add // 0' "$path") ||
			die "could not read the MCP servers of $path"
		[[ $count == 0 ]] || die "the temp home's $path holds $count MCP server(s) at user or local scope"
	done
}

# MCP config files above a scenario, which an agent may merge with its own,
# must be the ones the scenario wrote.
mcp_assert_ancestry() {
	local dir=$1 file up
	shift
	while :; do
		for file in .mcp.json opencode.json opencode.jsonc .opencode/opencode.json \
			.opencode/opencode.jsonc .codex/config.toml; do
			[[ -e $dir/$file ]] || continue
			in_list "$dir/$file" "$@" ||
				die "an MCP config file the scenario did not write is above it: $dir/$file"
		done
		# The top is where dirname changes nothing: / for the absolute path a
		# scenario has, and . for a relative one, which would otherwise never
		# reach /.
		up=$(dirname "$dir")
		[[ $up != "$dir" ]] || break
		dir=$up
	done
}

# mcp_handshake <log> [path]: loaded when the server saw initialize and
# tools/list, initialized when it saw only the first, none when neither, or
# when it never started. An http log is the run's, so it is read under the
# entry's path.
mcp_handshake() {
	[[ -s $1 ]] || {
		printf 'none\n'
		return
	}
	jq -rs --arg path "${2:-}" '
		[.[] | select(.event == "rpc" and ((.path // "") | startswith($path))) | .method] as $seen
		| if (any($seen[]; . == "initialize") | not) then "none"
		  elif (any($seen[]; . == "tools/list") | not) then "initialized"
		  else "loaded" end' "$1"
}

# mcp_starts <log>: how many times the stdio stand-in was started.
mcp_starts() {
	[[ -s $1 ]] || {
		printf '0\n'
		return
	}
	jq -rs '[.[] | select(.event == "start")] | length' "$1"
}

# mcp_arg <log>: what the agent made of the reference after --arg, the first
# time it started the server.
mcp_arg() {
	[[ -s $1 ]] || {
		printf '<not started>\n'
		return
	}
	jq -rs '[.[] | select(.event == "start")][0] as $start
		| if $start == null then "<not started>"
		  else ($start.argv | index("--arg")) as $at
		  | if $at == null then "<absent>" else $start.argv[$at + 1] // "<absent>" end end' "$1"
}

# mcp_env_of <log> <name>: that variable in the server's environment, the
# first time it was started.
mcp_env_of() {
	[[ -s $1 ]] || {
		printf '<not started>\n'
		return
	}
	jq -rs --arg name "$2" '[.[] | select(.event == "start")][0] as $start
		| if $start == null then "<not started>" else $start.env[$name] // "<unset>" end' "$1"
}

# mcp_http_path <log> <path>: the path, percent-decoded, of the first POST
# under that path: what the agent made of the reference in the URL.
mcp_http_path() {
	[[ -s $1 ]] || {
		printf '<no request>\n'
		return
	}
	jq -rs --arg path "$2" '[.[] | select(.event == "http" and .verb == "POST"
		and (.path | startswith($path))) | .path] | first // "<no request>"' "$1"
}

# mcp_http_header <log> <path> <header>: that header, by its lowercase name,
# in the first POST under that path.
mcp_http_header() {
	[[ -s $1 ]] || {
		printf '<no request>\n'
		return
	}
	jq -rs --arg path "$2" --arg header "$3" '[.[] | select(.event == "http" and .verb == "POST"
		and (.path | startswith($path)))] | if length == 0 then "<no request>"
		else .[0].headers[$header] // "<absent>" end' "$1"
}

# mcp_report <run> <log> [path]: what a stand-in saw, for the operator.
mcp_report() {
	local what
	if [[ ${3+set} ]]; then
		what="http ${3}: $(mcp_handshake "$2" "$3"), $(jq -rs --arg path "$3" \
			'[.[] | select(.event == "http" and (.path | startswith($path)))] | length' "$2" 2>/dev/null || printf 0) request(s)"
	else
		what="stdio ${2##*/}: $(mcp_handshake "$2"), started $(mcp_starts "$2") time(s)"
	fi
	say "  stand-in, $what"
}
