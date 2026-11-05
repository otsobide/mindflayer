#!/usr/bin/env python3
"""A stand-in MCP server for scripts/agent-probe (#18).

It answers just enough of MCP for an agent to load it, over stdio or over
HTTP on loopback, and writes down what the agent started it with and sent
it, one JSON object a line. A check then reads what the server saw, rather
than asking the model what it holds: whether the agent started the server at
all, and what each reference to an environment variable became on the way.

  mcp_server.py stdio --name NAME --log FILE [--arg VALUE]
  mcp_server.py http --name NAME --log FILE --port-file FILE

`--arg` does nothing but stand in the argument list, where an agent puts the
value it made of a reference. The HTTP server binds 127.0.0.1 on a port the
kernel picks and writes it to the port file once it listens; every entry a
run gives the agent reaches it under a path of its own (/<server>/...).

Standard library only, and run with `python3 -I`: inside the probe's
namespace there is /usr/bin/python3 and nothing installed for it.

What is kept of a value an agent supplied: the probe's own values all say
"probe" (probe-token-value, ${PROBE_MCP_TOKEN}, {env:PROBE_MCP_TOKEN}), and
those are kept whole, as is an empty one. Anything else, a variable of the
agent's own environment or a header it added, is kept as its length only,
so a credential never reaches the results. The argument list is kept whole:
the probe wrote every word of it, and the only words an agent changes are
references to the probe's own variables.
"""

import argparse
import json
import os
import sys
import threading
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

# What to answer an initialize that names no version. One that names a
# version is answered with that version: the client asked for it, so it
# speaks it, and the server has nothing of a later version to offer.
FALLBACK_VERSION = "2025-06-18"

# Headers that say how a client speaks, never who it is, kept whole.
PROTOCOL_HEADERS = {
    "accept",
    "content-length",
    "content-type",
    "host",
    "mcp-protocol-version",
    "mcp-session-id",
    "user-agent",
}

TOOL = {
    "name": "probe_echo",
    "description": "Agent probe tool. Never call it.",
    "inputSchema": {"type": "object", "properties": {}},
}

LISTS = {
    "prompts/list": "prompts",
    "resources/list": "resources",
    "resources/templates/list": "resourceTemplates",
}


def kept(value):
    """The value when it is the probe's own or empty, else its length."""
    if value == "" or "probe" in value.lower():
        return value
    return f"<withheld, {len(value)} characters>"


class Log:
    """One JSON object a line, appended and flushed as it happens: an agent
    may kill the server at any moment, and a buffered line would go with it,
    and every line after it."""

    def __init__(self, path, name):
        self.path = path
        self.name = name
        self.lock = threading.Lock()

    def write(self, event, **fields):
        record = {"event": event, "server": self.name, "pid": os.getpid(), **fields}
        line = json.dumps(record, sort_keys=True) + "\n"
        with self.lock, open(self.path, "a", encoding="utf-8") as file:
            file.write(line)


def result(ident, value):
    return {"jsonrpc": "2.0", "id": ident, "result": value}


def error(ident, code, message):
    return {"jsonrpc": "2.0", "id": ident, "error": {"code": code, "message": message}}


def answer(message):
    """The reply to one JSON-RPC message, or None for a notification, and
    for a reply the client sent, since this server asks it nothing."""
    if not isinstance(message, dict):
        return error(None, -32600, "not a JSON-RPC message")
    method = message.get("method")
    if "id" not in message or not isinstance(method, str):
        return None
    ident = message["id"]
    params = message.get("params")
    if method == "initialize":
        version = params.get("protocolVersion") if isinstance(params, dict) else None
        return result(
            ident,
            {
                "protocolVersion": version or FALLBACK_VERSION,
                "capabilities": {"tools": {"listChanged": False}},
                "serverInfo": {"name": "agent-probe", "version": "1"},
            },
        )
    if method == "ping":
        return result(ident, {})
    if method == "tools/list":
        return result(ident, {"tools": [TOOL]})
    if method == "tools/call":
        text = {"type": "text", "text": "PROBE-MCP-TOOL-CALLED"}
        return result(ident, {"content": [text], "isError": False})
    if method in LISTS:
        return result(ident, {LISTS[method]: []})
    return error(ident, -32601, f"method not found: {method}")


def methods(message):
    """The method of every request and notification in a message."""
    batch = message if isinstance(message, list) else [message]
    return [m["method"] for m in batch if isinstance(m, dict) and isinstance(m.get("method"), str)]


def replies(message):
    """What to send back for a message, a batch answered as a batch, or None
    when nothing in it asks for an answer."""
    batch = message if isinstance(message, list) else [message]
    answered = [reply for reply in map(answer, batch) if reply is not None]
    if not answered:
        return None
    return answered if isinstance(message, list) else answered[0]


def serve_stdio(log):
    log.write(
        "start",
        transport="stdio",
        argv=sys.argv[1:],
        cwd=os.getcwd(),
        env={name: kept(value) for name, value in os.environ.items() if name.startswith("PROBE_")},
        env_names=sorted(os.environ),
    )
    out = sys.stdout.buffer
    # MCP's stdio transport is one message a line, with no newline inside one.
    for raw in sys.stdin.buffer:
        line = raw.strip()
        if not line:
            continue
        try:
            message = json.loads(line)
        except ValueError:
            log.write("unparsed", bytes=len(line))
            continue
        for method in methods(message):
            log.write("rpc", method=method)
        reply = replies(message)
        if reply is not None:
            out.write(json.dumps(reply).encode("utf-8") + b"\n")
            out.flush()
    log.write("end")


class Handler(BaseHTTPRequestHandler):
    """MCP's Streamable HTTP, as little of it as a client needs: requests
    come by POST and are answered as JSON, never as an event stream, and a
    GET for the stream the server may offer is refused, which the transport
    allows. Nothing asks for a session or for OAuth, so a client that
    connects never needs either."""

    protocol_version = "HTTP/1.1"
    server_version = "agent-probe"

    def log_message(self, format, *args):
        pass

    def body(self):
        if self.headers.get("Transfer-Encoding", "").lower() == "chunked":
            chunks = []
            while True:
                size = int(self.rfile.readline().split(b";")[0].strip() or b"0", 16)
                if size == 0:
                    while self.rfile.readline() not in (b"\r\n", b"\n", b""):
                        pass
                    return b"".join(chunks)
                chunks.append(self.rfile.read(size))
                self.rfile.readline()
        length = int(self.headers.get("Content-Length") or 0)
        return self.rfile.read(length) if length else b""

    def record(self, found):
        path = urllib.parse.unquote(urllib.parse.urlsplit(self.path).path)
        headers = {}
        for name, value in self.headers.items():
            lower = name.lower()
            headers[lower] = value if lower in PROTOCOL_HEADERS else kept(value)
        log = self.server.probe_log
        log.write("http", verb=self.command, raw_path=self.path, path=path, headers=headers)
        for method in found:
            log.write("rpc", path=path, method=method)

    def reply(self, status, payload=None, headers=()):
        data = b"" if payload is None else json.dumps(payload).encode("utf-8")
        self.send_response(status)
        if payload is not None:
            self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        for name, value in headers:
            self.send_header(name, value)
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        try:
            message = json.loads(self.body())
        except ValueError:
            self.record([])
            self.reply(400, error(None, -32700, "parse error"))
            return
        self.record(methods(message))
        reply = replies(message)
        if reply is None:
            self.reply(202)
        else:
            self.reply(200, reply)

    def do_GET(self):
        self.record([])
        # No OAuth metadata: a 404 says there is none, where a 405 would
        # read as a broken authorization server.
        if urllib.parse.urlsplit(self.path).path.startswith("/.well-known/"):
            self.reply(404)
        else:
            self.reply(405, headers=[("Allow", "POST")])

    def do_DELETE(self):
        self.record([])
        self.reply(405, headers=[("Allow", "POST")])


def serve_http(log, port_file):
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    server.probe_log = log
    port = server.server_address[1]
    log.write("start", transport="http", port=port)
    # Written whole and then renamed, so a reader never sees half a number.
    partial = port_file + ".partial"
    with open(partial, "w", encoding="utf-8") as file:
        file.write(f"{port}\n")
    os.replace(partial, port_file)
    server.serve_forever()


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("transport", choices=["stdio", "http"])
    parser.add_argument("--name", required=True)
    parser.add_argument("--log", required=True)
    parser.add_argument("--arg", default=None)
    parser.add_argument("--port-file")
    options = parser.parse_args()
    log = Log(options.log, options.name)
    if options.transport == "stdio":
        serve_stdio(log)
    else:
        if not options.port_file:
            parser.error("http needs --port-file")
        serve_http(log, options.port_file)


if __name__ == "__main__":
    main()
