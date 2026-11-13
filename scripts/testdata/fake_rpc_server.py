"""A local fake NEAR RPC for scripts/test-wait-for-coin.sh. It answers every POST by MODE and
counts the requests in COUNT_FILE. Modes: 403, 429, 500, unknown, launch (UNKNOWN_ACCOUNT twice,
then the fc_rules result), retryafter (one 429 with Retry-After: 2, then the fc_rules result). It prints its port on stdout and serves until killed."""
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
        status, body, headers = 200, "", {}
        if mode in ("403", "429", "500"):
            status = int(mode)
        elif mode == "retryafter" and count == 1:
            status, headers = 429, {"Retry-After": "2"}
        elif mode == "unknown":
            body = json.dumps({"jsonrpc": "2.0", "id": "fc", "result": {"something": "else"}})
        elif mode == "launch" and count <= 2:
            body = json.dumps({"jsonrpc": "2.0", "error": {"name": "HANDLER_ERROR", "cause": {"info": {}, "name": "UNKNOWN_ACCOUNT"}, "code": -32000, "message": "Server error", "data": "account coin.testnet does not exist while viewing"}, "id": "fc"}, separators=(",", ":"))
        else:
            # The field order of a real testnet answer: block_hash, block_height, logs, result.
            body = json.dumps({"jsonrpc": "2.0", "result": {"block_hash": "x", "block_height": 1, "logs": [], "result": [123, 125]}, "id": "fc"}, separators=(",", ":"))
        raw = body.encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        for name, value in headers.items():
            self.send_header(name, value)
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def log_message(self, *args):
        pass


server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
print(server.server_address[1], flush=True)
server.serve_forever()
