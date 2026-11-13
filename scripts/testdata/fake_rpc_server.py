"""A local fake NEAR RPC for scripts/test-wait-for-coin.sh. It answers every POST by MODE and
counts the requests in COUNT_FILE. Modes: 403, 429, 500, unknown, launch (UNKNOWN_ACCOUNT twice,
then the fc_rules result). It prints its port on stdout and serves until killed."""
import http.server
import json
import sys

mode, count_file = sys.argv[1], sys.argv[2]
count = 0


class Handler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        global count
        self.rfile.read(int(self.headers.get("Content-Length", 0)))
        count += 1
        with open(count_file, "w") as f:
            f.write(str(count))
        status, body = 200, ""
        if mode in ("403", "429", "500"):
            status = int(mode)
        elif mode == "unknown":
            body = json.dumps({"jsonrpc": "2.0", "id": "fc", "result": {"something": "else"}})
        elif mode == "launch" and count <= 2:
            body = json.dumps({"jsonrpc": "2.0", "id": "fc", "error": {"name": "HANDLER_ERROR", "cause": {"name": "UNKNOWN_ACCOUNT"}}})
        else:
            body = json.dumps({"jsonrpc": "2.0", "id": "fc", "result": {"result": [123, 125], "logs": []}}, separators=(",", ":"))
        raw = body.encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def log_message(self, *args):
        pass


server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
print(server.server_address[1], flush=True)
server.serve_forever()
