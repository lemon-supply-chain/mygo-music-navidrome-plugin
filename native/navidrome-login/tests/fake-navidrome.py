#!/usr/bin/env python3
"""A stand-in for a Navidrome server that only speaks /rest/ping.view."""
import hashlib
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

USER = sys.argv[1]
PASSWORD = sys.argv[2]
PORT = int(sys.argv[3])


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        url = urlparse(self.path)
        query = {key: values[0] for key, values in parse_qs(url.query).items()}
        if not url.path.endswith("/rest/ping.view"):
            self.send_response(404)
            self.end_headers()
            return
        expected = hashlib.md5((PASSWORD + query.get("s", "")).encode()).hexdigest()
        if query.get("u") == USER and query.get("t") == expected:
            body = {"subsonic-response": {"status": "ok", "version": "1.16.1"}}
        else:
            body = {
                "subsonic-response": {
                    "status": "failed",
                    "error": {"code": 40, "message": "Wrong username or password"},
                }
            }
        data = json.dumps(body).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *_):
        pass


ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()