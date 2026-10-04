#!/usr/bin/env python3
"""Loopback HTTP adapter for Fastino GLiNER2.5-Decide."""

import argparse
import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

MAX_BODY = 2 * 1024 * 1024
MAX_LABELS = 1_000
MAX_TEXT = 1_000_000


def load_model(model_id: str):
    from gliner2 import AutoExtractor

    return AutoExtractor.from_pretrained(model_id)


class Handler(BaseHTTPRequestHandler):
    model = None

    def log_message(self, _format, *_args):
        return

    def respond(self, status: int, payload: dict):
        body = json.dumps(payload, separators=(",", ":")).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        if self.path != "/v1/labels":
            self.respond(404, {"error": "not_found"})
            return
        if self.headers.get_content_type() != "application/json":
            self.respond(415, {"error": "content_type"})
            return
        try:
            length = int(self.headers.get("Content-Length", ""))
        except ValueError:
            self.respond(400, {"error": "request"})
            return
        if length <= 0 or length > MAX_BODY:
            self.respond(413, {"error": "request_size"})
            return
        try:
            request = json.loads(self.rfile.read(length))
            title = request["title"]
            description = request["description"]
            labels = request["labels"]
            if (not isinstance(title, str) or not isinstance(description, str)
                    or len(title) + len(description) > MAX_TEXT
                    or not isinstance(labels, list) or not 1 <= len(labels) <= MAX_LABELS):
                raise ValueError
            names = []
            definitions = {}
            for label in labels:
                if not isinstance(label, dict):
                    raise ValueError
                name = label.get("name")
                detail = label.get("description")
                if not isinstance(name, str) or not name.strip() or (detail is not None and not isinstance(detail, str)):
                    raise ValueError
                if name in definitions:
                    raise ValueError
                names.append(name)
                definitions[name] = detail or name
        except (KeyError, TypeError, ValueError, json.JSONDecodeError):
            self.respond(400, {"error": "request"})
            return

        try:
            text = f"Title: {title}\nDescription:\n{description}"
            schema = self.model.create_schema().classification(
                "labels", definitions, multi_label=True, cls_threshold=0.0
            )
            result = self.model.extract(text, schema, include_confidence=True)
            raw = result.get("labels")
            if not isinstance(raw, list):
                raise ValueError
            scores = {entry["label"]: entry["confidence"] for entry in raw
                      if isinstance(entry, dict) and "label" in entry and "confidence" in entry}
            suggestions = []
            for name in names:
                probability = scores.get(name)
                if isinstance(probability, bool) or not isinstance(probability, (int, float)) or not 0 <= probability <= 1:
                    raise ValueError
                suggestions.append({"label": name, "probability": probability})
        except Exception:
            self.respond(503, {"error": "inference"})
            return
        self.respond(200, {"suggestions": suggestions})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--model", default=os.environ.get("GLINER_MODEL", "fastino/GLiNER2.5-Decide"))
    args = parser.parse_args()
    if args.host not in {"127.0.0.1", "::1", "localhost"}:
        parser.error("the decision service must bind to loopback")
    Handler.model = load_model(args.model)
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    server.serve_forever()


if __name__ == "__main__":
    main()
