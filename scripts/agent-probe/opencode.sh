# OpenCode's checks for probe.sh, which sources this file: opencode_prepare
# runs outside the namespace, where the real home is visible, and
# opencode_checks inside it. docs/agents.md says what each check found.
#
# Only the MCP checks of #18 so far. Where OpenCode reads skills and rules
# stays docs-only in docs/agents.md, as #17 left it, until runs about them
# join opencode_runs here.
#
# Both lines are run: V2 as installed, and V1 as 1.19.4, the version D-12
# names, fetched from the npm registry into the run's temp dir, since both
# lines install a binary named opencode. Each run gives a project an
# opencode.json holding one local and one remote server, with {env:NAME} in
# the command, the environment, the url and the headers, and asks
# `opencode mcp list`, which connects to every server it lists and says how
# each went. No run calls a model, so no login is copied in, and none costs
# anything. What loaded is read from what the stand-ins of mcp.sh saw, and
# the list's own word for each server is kept beside it.

opencode_runs=(opencode-mcp-v2 opencode-mcp-v1-shape opencode-mcp-service opencode-mcp-1-19)
opencode_v2_runs=(opencode-mcp-v2 opencode-mcp-v1-shape opencode-mcp-service)
opencode_v1=1.19.4

opencode_prepare() {
	# Every run is an MCP one, and each bounds what it asks with timeout.
	namespace_tool python3 "for the MCP checks' stand-in server"
	namespace_tool timeout
	if any_selected "${opencode_v2_runs[@]}"; then
		local bin
		bin=$(opencode_native "${PROBE_OPENCODE_BIN:-$(command -v opencode || true)}") || exit 2
		expose+=("$bin")
		namespace_env+=(PROBE_OPENCODE_BIN "$bin")
		say "opencode: V2 at $bin, its version asked inside"
	fi
	if selected opencode-mcp-1-19; then
		opencode_fetch_v1
	fi
	say "opencode: no login copied: its runs list servers and call no model"
}

# opencode_native <path>: the OpenCode binary that path stands for: itself
# when it is one, or the one its npm package ships when the path is the
# package's launcher script, as node_modules/.bin/opencode is in a mise
# install. Asking a stand-in what it is would run it out here, against the
# real home, so a binary is told by its name and its first bytes, and its
# version is asked inside.
opencode_native() {
	local bin=$1 shipped
	[[ -n $bin ]] || die "no opencode on PATH (or set PROBE_OPENCODE_BIN)"
	bin=$(readlink -f -- "$bin")
	if [[ $(head -c 2 -- "$bin") == '#!' ]]; then
		shipped=$(dirname "$bin")/../@opencode/cli/bin/opencode.exe
		[[ -f $shipped ]] ||
			die "$bin is a launcher script with no @opencode/cli package beside it: set PROBE_OPENCODE_BIN to the opencode binary itself"
		bin=$(readlink -f -- "$shipped")
	fi
	[[ (${bin##*/} == opencode || ${bin##*/} == opencode.exe) && $(head -c 4 -- "$bin") == $'\x7fELF' ]] ||
		die "$bin is not the opencode binary: set PROBE_OPENCODE_BIN to it (in an npm install, node_modules/@opencode/cli/bin/opencode.exe)"
	printf '%s\n' "$bin"
}

# OpenCode 1.19.4, the V1 line. npx opencode-ai@1.19.4 would run its package's
# postinstall, which copies the binary of the package for this platform into
# place; this takes that package itself, with npm pack, which runs nothing,
# and makes the same choice of it: baseline without AVX2, musl on musl. Its
# cache is the run's, so ~/.npm is left alone.
opencode_fetch_v1() {
	command -v npm >/dev/null || die "needs npm on PATH to fetch OpenCode $opencode_v1 for opencode-mcp-1-19"
	command -v tar >/dev/null || die "needs tar on PATH to unpack OpenCode $opencode_v1"
	local arch pkg libc dir=$run/opencode-$opencode_v1
	case $(uname -m) in
	x86_64) arch=x64 ;;
	aarch64 | arm64) arch=arm64 ;;
	*) die "no OpenCode $opencode_v1 package for $(uname -m)" ;;
	esac
	pkg=opencode-linux-$arch
	if [[ $arch == x64 ]] && ! grep -qw avx2 /proc/cpuinfo; then pkg+=-baseline; fi
	# What ldd says is read whole and looked at after: musl's says so on
	# stderr and exits 1, which fails a pipeline into grep under pipefail
	# whatever grep found. Alpine, which the postinstall takes for musl too,
	# is told by its release file.
	libc=$(ldd --version 2>&1) || true
	if [[ -f /etc/alpine-release || ${libc,,} == *musl* ]]; then pkg+=-musl; fi
	mkdir -p -- "$dir"
	(cd "$dir" && npm pack "$pkg@$opencode_v1" --cache "$run/npm-cache" \
		--update-notifier=false --loglevel=error >/dev/null) ||
		die "npm could not fetch $pkg@$opencode_v1"
	tar -xzf "$dir/$pkg-$opencode_v1.tgz" -C "$dir" package/bin/opencode ||
		die "$pkg-$opencode_v1.tgz holds no package/bin/opencode"
	rm -rf -- "$run/npm-cache" "$dir/$pkg-$opencode_v1.tgz"
	[[ $(head -c 4 -- "$dir/package/bin/opencode") == $'\x7fELF' ]] ||
		die "$pkg@$opencode_v1's package/bin/opencode is not a binary"
	namespace_env+=(PROBE_OPENCODE_V1_BIN "$mount_root/opencode-$opencode_v1/package/bin/opencode")
	say "opencode: $pkg@$opencode_v1 fetched from the npm registry into the run's temp dir"
}

# The version a binary answers, as its first x.y.z.
opencode_version() {
	timeout 60 "$1" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -n 1 || true
}

opencode_checks() {
	say ""
	if [[ -n ${PROBE_OPENCODE_BIN:-} ]]; then
		opencode_v2_version=$(opencode_version "$PROBE_OPENCODE_BIN")
		[[ $opencode_v2_version == 2.* ]] ||
			die "PROBE_OPENCODE_BIN is OpenCode ${opencode_v2_version:-of no version}, not the V2 line its runs are about"
		say "opencode: V2 $opencode_v2_version"
	fi
	if [[ -n ${PROBE_OPENCODE_V1_BIN:-} ]]; then
		local version
		version=$(opencode_version "$PROBE_OPENCODE_V1_BIN")
		[[ $version == "$opencode_v1" ]] ||
			die "the OpenCode fetched as $opencode_v1 says it is ${version:-of no version}"
		say "opencode: V1 $version"
	fi
	local name function
	for name in "${opencode_runs[@]}"; do
		selected "$name" || continue
		function=opencode_${name#opencode-}
		"${function//-/_}"
	done
}

# opencode_scenario <run> <repo> <v2|v1>: a fresh repository whose
# opencode.json holds the two servers in that shape, V2's mcp.servers or
# V1's entries straight under mcp, and the http stand-in they reach. OpenCode
# fills {env:NAME} in the text of the file before it reads it, so a reference
# works in any value: the command's first word, an argument, the environment,
# the url and the headers each hold one.
opencode_scenario() {
	local run=$1 repo=$2 command servers
	mcp_need_python
	new_repo "$repo"
	mcp_http_start "$run"
	command=$(mcp_stdio_json "$run" probe-stdio --arg '{env:PROBE_MCP_ARG}' |
		jq -c '["{env:PROBE_MCP_PYTHON}"] + .')
	servers=$(jq -n --argjson command "$command" \
		--arg url "http://127.0.0.1:$mcp_port/probe-http/{env:PROBE_MCP_URL_PART}" \
		'{"probe-stdio": {type: "local", command: $command,
			environment: {PROBE_MCP_ENV: "{env:PROBE_MCP_ENV_SOURCE}"}},
		"probe-http": {type: "remote", url: $url, headers: {
			"X-Probe-Header": "{env:PROBE_MCP_HEADER}",
			Authorization: "Bearer {env:PROBE_MCP_TOKEN}"}}}')
	if [[ $3 == v2 ]]; then
		jq -n --argjson servers "$servers" '{mcp: {servers: $servers}}' >"$repo/opencode.json"
	else
		jq -n --argjson servers "$servers" '{mcp: ($servers | map_values(. + {enabled: true}))}' \
			>"$repo/opencode.json"
	fi
	mcp_keep "$run" "$repo" opencode.json
	# Auto-update and the models.dev fetch off: a list needs neither, and
	# the run should reach nothing but the stand-ins and npm, for plugins.
	# Those two and the command's first word are what a background server
	# started apart holds (opencode_mcp_service), without the five
	# variables the list is given.
	opencode_apart_env=(OPENCODE_DISABLE_AUTOUPDATE=1 OPENCODE_DISABLE_MODELS_FETCH=1
		"PROBE_MCP_PYTHON=$mcp_python")
	opencode_env=("${opencode_apart_env[@]}" "${mcp_env[@]}")
	assert_clean_home
	mcp_assert_clean_home
	assert_ancestry "$repo"
	mcp_assert_ancestry "$repo" "$repo/opencode.json"
}

# opencode_list <name> <dir> <binary> [options...]: `opencode mcp list` there.
opencode_list() {
	local name=$1 dir=$2 bin=$3 status=0
	shift 3
	say ""
	say "== $name"
	{
		printf 'cd %q\n' "$dir"
		printf '%s ' "${opencode_env[@]}"
		printf '%q mcp list' "$bin"
		(($# == 0)) || printf ' %q' "$@"
		printf '\n'
	} | tee "$results/$name.command" | sed 's/^/  $ /'
	(cd "$dir" && env "${opencode_env[@]}" timeout --kill-after=10 120 "$bin" mcp list "$@") \
		</dev/null >"$results/$name.list" 2>"$results/$name.stderr" || status=$?
	say "  exit $status; the list:"
	sed 's/^/    /' "$results/$name.list"
	if [[ $status != 0 ]]; then
		say "  stderr:"
		head -n 20 -- "$results/$name.stderr" | sed 's/^/    /'
	fi
}

# opencode_status <name> <server>: the list's word for that server.
opencode_status() {
	local line word
	line=$(sed 's/\x1b\[[0-9;]*m//g' "$results/$1.list" 2>/dev/null | grep -F -- "$2" | head -n 1) || line=
	for word in 'needs authentication' 'needs client registration' 'not initialized' \
		connected failed disabled pending; do
		[[ $line == *"$word"* ]] || continue
		printf '%s\n' "$word"
		return
	done
	if [[ -n $line ]]; then
		printf 'other: %s\n' "$line"
	else
		printf 'absent\n'
	fi
}

# opencode_expect <run> <what the shape shows>: every verdict a run gives,
# its http stand-in stopped first.
opencode_expect() {
	local run=$1 stdio=$results/$1.probe-stdio.jsonl http=$results/$1.http.jsonl
	mcp_http_stop
	mcp_report "$run" "$stdio"
	mcp_report "$run" "$http" /probe-http/
	verdict $run loaded "$(mcp_handshake "$stdio")" 'probe-stdio stand-in' "$2: its local entry loads"
	verdict $run probe-arg-value "$(mcp_arg "$stdio")" 'stdio argument' 'D-11: {env:} in command expands, the first word too'
	verdict $run probe-env-value "$(mcp_env_of "$stdio" PROBE_MCP_ENV)" 'stdio environment' 'D-11: {env:} in environment expands'
	verdict $run loaded "$(mcp_handshake "$http" /probe-http/)" 'probe-http stand-in' "$2: its remote entry loads"
	verdict $run /probe-http/probe-url-value "$(mcp_http_path "$http" /probe-http/)" 'http path' 'D-11: {env:} in url expands'
	verdict $run probe-header-value "$(mcp_http_header "$http" /probe-http/ x-probe-header)" 'header X-Probe-Header' 'D-11: {env:} in headers expands'
	verdict $run 'Bearer probe-token-value' "$(mcp_http_header "$http" /probe-http/ authorization)" 'header Authorization' 'D-11: and inside a longer value'
	observe $run 'probe-stdio in the list' "$(opencode_status $run probe-stdio)" "$2: the list's own word"
	observe $run 'probe-http in the list' "$(opencode_status $run probe-http)" "$2: the list's own word"
}

# D-12: V2's own shape in V2, on a private server, which runs in the list's
# own process and environment.
opencode_mcp_v2() {
	local run=opencode-mcp-v2
	opencode_scenario $run "$stage/$run" v2
	opencode_private_list $run "$stage/$run"
	opencode_expect $run "D-12: mcp.servers in $opencode_v2_version"
}

# D-12: V1's shape in V2, which #92 writes when a file already uses it.
opencode_mcp_v1_shape() {
	local run=opencode-mcp-v1-shape
	opencode_scenario $run "$stage/$run" v1
	opencode_private_list $run "$stage/$run"
	opencode_expect $run "D-12: V1's mcp.<name> in $opencode_v2_version"
}

# opencode_service <name> <what> <dir> <arguments...>: `opencode service
# <arguments>` from that folder, its output kept as <name>.<what> and its
# exit status left in opencode_service_status. Each runs without the five
# variables (opencode_apart_env), so a server it starts holds none of them.
opencode_service() {
	local name=$1 what=$2 dir=$3
	shift 3
	{
		printf 'cd %q\n' "$dir"
		printf '%s ' "${opencode_apart_env[@]}"
		printf '%q service' "$PROBE_OPENCODE_BIN"
		printf ' %q' "$@"
		printf '\n'
	} >"$results/$name.$what.command"
	opencode_service_status=0
	# A start waits for the server to answer, two minutes at most.
	(cd "$dir" && env "${opencode_apart_env[@]}" timeout --kill-after=10 150 \
		"$PROBE_OPENCODE_BIN" service "$@") </dev/null >"$results/$name.$what" 2>&1 ||
		opencode_service_status=$?
	say "  \$ opencode service $*: exit $opencode_service_status"
	if [[ $opencode_service_status != 0 ]]; then
		head -n 5 -- "$results/$name.$what" | sed 's/^/    /'
	fi
}

# opencode_own_port <run> <dir>: the temp home's background server given a
# port nothing holds. The namespace shares the host's network, and a V2
# server takes its channel's fixed port, 49374, unless its settings name
# another; the operator's own OpenCode may hold that one, which would fail
# every reading here for a reason that has nothing to do with MCP, and keep
# the operator's own server from starting while the probe's ran. `opencode
# service set port` writes the temp home's service settings alone. False,
# with a mismatch said and the run's http stand-in stopped, when it fails.
opencode_own_port() {
	local port
	port=$("$mcp_python" -I -c 'import socket
with socket.socket() as s:
    s.bind(("127.0.0.1", 0))
    print(s.getsockname()[1])') || port=
	opencode_service "$1" port "$2" set port "$port"
	verdict "$1" 0 "$opencode_service_status" 'service set port' 'the background server gets a port of its own'
	[[ $opencode_service_status == 0 ]] && return 0
	mcp_http_stop
	return 1
}

# opencode_unsettled <name>: whether the list kept as <name> is one to ask
# again. One asked before a server has connected says pending; and the first
# list a V2 server is asked for a folder says no server is configured at
# all, since that request is what boots the folder's services, and its MCP
# servers are read from the configuration in the background while the
# answer goes out (2.2.5, seen in a temp home: the second ask lists them).
opencode_unsettled() {
	grep -q -e pending -e 'No MCP servers configured' -- "$results/$1.list"
}

# opencode_service_list <name> <dir> [options...]: the list through the
# background server, which it starts when none is running, or through the
# server the options name, asked again, a few seconds apart, while it is
# unsettled (opencode_unsettled). Stopping the server is the caller's.
opencode_service_list() {
	local name=$1 dir=$2 attempt
	shift 2
	for attempt in 1 2 3 4 5; do
		opencode_list "$name" "$dir" "$PROBE_OPENCODE_BIN" "$@"
		opencode_unsettled "$name" || break
		sleep 3
	done
	say "  asked $attempt time(s)"
}

# opencode_private_list <name> <dir>: the list from a private server of its
# own, started from that folder in the list's environment, as `mcp list
# --standalone` starts one: a server in the list's own environment, which
# nothing else asks. `--standalone` itself cannot be used: its server lives
# for one list, so every answer is a first one, which says no server is
# configured (opencode_unsettled), and 2.1.3 has no such flag. The password
# is the run's own, for this server alone, and it is stopped once the list
# has settled.
#
# 2.1.3's list takes no `--server` either, so there the list reaches the
# temp home's background server, which it starts itself and which then
# holds the list's environment, as a private server does; on a port of its
# own (opencode_own_port), since the channel's fixed one may be the
# operator's own server, and stopped after.
opencode_private_list() {
	local name=$1 dir=$2 port pid waited=0 help
	# Read whole, then looked at: grep -q stopping early would fail the
	# pipeline under pipefail, and read as no such flag.
	help=$(cd "$dir" && env "${opencode_env[@]}" timeout 60 "$PROBE_OPENCODE_BIN" mcp list --help 2>&1) || true
	if [[ $help != *--server* ]]; then
		say "  (this list takes no --server: through the background server it starts)"
		if opencode_own_port "$name" "$dir"; then
			opencode_service_list "$name" "$dir"
			opencode_service "$name" stop "$dir" stop
			verdict "$name" 0 "$opencode_service_status" 'service stop' 'the server the list started is gone before the next run'
		else
			: >"$results/$name.list"
		fi
		return 0
	fi
	port=$("$mcp_python" -I -c 'import socket
with socket.socket() as s:
    s.bind(("127.0.0.1", 0))
    print(s.getsockname()[1])')
	local opencode_env=("${opencode_env[@]}" OPENCODE_SERVER_PASSWORD=probe-run-only)
	(cd "$dir" && exec env "${opencode_env[@]}" timeout --kill-after=10 300 \
		"$PROBE_OPENCODE_BIN" serve --port "$port") </dev/null >"$results/$name.serve" 2>&1 &
	pid=$!
	until grep -q 'server listening' -- "$results/$name.serve"; do
		if ((waited >= 60)) || ! kill -0 "$pid" 2>/dev/null; then
			say "  the private server did not start:"
			head -n 5 -- "$results/$name.serve" | sed 's/^/    /'
			kill "$pid" 2>/dev/null || true
			wait "$pid" 2>/dev/null || true
			: >"$results/$name.list"
			return 0
		fi
		sleep 1
		waited=$((waited + 1))
	done
	opencode_service_list "$name" "$dir" --server "http://127.0.0.1:$port"
	kill "$pid" 2>/dev/null || true
	wait "$pid" 2>/dev/null || true
}

# opencode_record <run> <what the shape shows>: what a run that asks whose
# environment fills {env:} gives, its http stand-in stopped first. Both
# servers are expected to load whichever it is, since the server started
# apart holds the command's first word too; what each reference became is
# recorded, and how many of the five took the list's values.
opencode_record() {
	local run=$1 stdio=$results/$1.probe-stdio.jsonl http=$results/$1.http.jsonl
	local stdio_loaded http_loaded i expanded=0 whose got want
	mcp_http_stop
	mcp_report "$run" "$stdio"
	mcp_report "$run" "$http" /probe-http/
	stdio_loaded=$(mcp_handshake "$stdio")
	http_loaded=$(mcp_handshake "$http" /probe-http/)
	got=("$(mcp_arg "$stdio")" "$(mcp_env_of "$stdio" PROBE_MCP_ENV)"
		"$(mcp_http_path "$http" /probe-http/)"
		"$(mcp_http_header "$http" /probe-http/ x-probe-header)"
		"$(mcp_http_header "$http" /probe-http/ authorization)")
	want=(probe-arg-value probe-env-value /probe-http/probe-url-value
		probe-header-value 'Bearer probe-token-value')
	for i in "${!want[@]}"; do
		if [[ ${got[i]} == "${want[i]}" ]]; then expanded=$((expanded + 1)); fi
	done
	if [[ $stdio_loaded == none && $http_loaded == none ]]; then
		whose='neither server started'
	elif ((expanded == 5)); then
		whose="the list's: 5 of 5 expanded"
	elif ((expanded == 0)); then
		whose="not the list's: 0 of 5 expanded"
	else
		whose="mixed: $expanded of 5 expanded"
	fi
	verdict $run loaded "$stdio_loaded" 'probe-stdio stand-in' "$2: its local entry loads"
	observe $run 'stdio argument' "${got[0]}" "D-11: {env:} in command, filled from whose environment"
	observe $run 'stdio environment' "${got[1]}" 'D-11: {env:} in environment, the same'
	verdict $run loaded "$http_loaded" 'probe-http stand-in' "$2: its remote entry loads"
	observe $run 'http path' "${got[2]}" 'D-11: {env:} in url, the same'
	observe $run 'header X-Probe-Header' "${got[3]}" 'D-11: {env:} in headers, the same'
	observe $run 'header Authorization' "${got[4]}" 'D-11: and inside a longer value'
	observe $run 'whose environment' "$whose" "D-11, #92: the list's, or the server's own"
	observe $run 'probe-stdio in the list' "$(opencode_status $run probe-stdio)" "$2: the list's own word"
	observe $run 'probe-http in the list' "$(opencode_status $run probe-http)" "$2: the list's own word"
}

# D-11: V2's shape in V2 through its background server, the way a person runs
# OpenCode. Whether that server fills {env:} from the environment of the
# client that asks, or from its own, is the question, and a server that
# holds the variables cannot tell the two apart. So the first reading starts
# the server apart, with `opencode service start` and none of the five
# variables, as one started at login or from another shell would be, and
# asks the list from the project with all five: the stand-ins record which
# environment filled each reference. The second, <run>.inherited, has the
# list start the server itself, which then holds the list's variables, and
# every reference is expected to expand there.
opencode_mcp_service() {
	local run=opencode-mcp-service inherited=opencode-mcp-service.inherited
	opencode_scenario $run "$stage/$run" v2
	opencode_own_port $run "$stage/$run" || return 0
	opencode_service $run start "$stage/$run" start
	verdict $run 0 "$opencode_service_status" 'service start, apart' 'the server starts without the five variables'
	# A list with no server to ask would start one from its own
	# environment, and answer the question wrongly.
	if [[ $opencode_service_status == 0 ]]; then
		opencode_service_list $run "$stage/$run"
	fi
	opencode_service $run stop "$stage/$run" stop
	# Still running, it would answer the second reading in its place.
	verdict $run 0 "$opencode_service_status" 'service stop' 'the server started apart is gone before the next reading'
	opencode_record $run "D-12: mcp.servers through $opencode_v2_version's background server, started apart"

	opencode_scenario $inherited "$stage/$run-inherited" v2
	opencode_service_list $inherited "$stage/$run-inherited"
	opencode_service $inherited stop "$stage/$run-inherited" stop
	opencode_expect $inherited "D-12: mcp.servers through $opencode_v2_version's background server, started by the list"
}

# D-12: V2's shape in 1.19.4, which normalizes it since 1.18.24.
opencode_mcp_1_19() {
	local run=opencode-mcp-1-19
	opencode_scenario $run "$stage/$run" v2
	# Data, state and cache folders of its own: V2's runs leave their
	# database in the temp home's, which 1.19.4 refuses to open ("Database
	# is not empty and has no session table"). HOME and the config folders
	# stay the temp home's, which the clean-home checks hold empty.
	local opencode_env=("${opencode_env[@]}"
		"XDG_DATA_HOME=$mount_root/home-1-19/.local/share"
		"XDG_STATE_HOME=$mount_root/home-1-19/.local/state"
		"XDG_CACHE_HOME=$mount_root/home-1-19/.cache")
	opencode_list $run "$stage/$run" "$PROBE_OPENCODE_V1_BIN" --print-logs --log-level DEBUG
	opencode_expect $run "D-12: mcp.servers in $opencode_v1"
}
