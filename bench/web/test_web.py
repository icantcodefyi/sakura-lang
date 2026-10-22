"""Tests for bench/web/web.py's checks, parsing and process handling (no load tool, no servers
built): `python3 -m unittest discover -s bench/web`."""

import http.server
import json
import os
import signal
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import web  # noqa: E402

DATE = "Thu, 22 Oct 2026 17:12:19 GMT"


def headers(endpoint, **extra):
    h = {"content-type": "application/json" if endpoint == "json" else "text/plain", "server": "test",
         "date": DATE, "content-length": str(len(web.BODY[endpoint]))}
    h.update({k.replace("_", "-"): v for k, v in extra.items()})
    return {k: v for k, v in h.items() if v is not None}


def fails(problems):
    return [m for level, m in problems if level == "fail"]


class Checks(unittest.TestCase):
    def test_conforming_responses_have_no_problems(self):
        for endpoint, body in web.BODY.items():
            self.assertEqual(web.check_response(endpoint, 200, headers(endpoint), body), [], endpoint)

    def test_a_utf8_charset_is_allowed_and_another_type_is_not(self):
        h = headers("json", content_type="application/json; charset=UTF-8")
        self.assertEqual(web.check_response("json", 200, h, web.BODY["json"]), [])
        h = headers("plaintext", content_type="text/html")
        self.assertIn("content-type", fails(web.check_response("plaintext", 200, h, web.BODY["plaintext"]))[0])
        h = headers("plaintext", content_type="text/plain; charset=latin1")
        self.assertTrue(fails(web.check_response("plaintext", 200, h, web.BODY["plaintext"])))

    def test_server_date_and_content_type_are_required(self):
        for name in ("server", "date", "content_type"):
            h = headers("json", **{name: None})
            msgs = fails(web.check_response("json", 200, h, web.BODY["json"]))
            self.assertEqual(msgs, [f"required header missing: {name.replace('_', '-')}"], name)

    def test_a_length_or_chunked_encoding_is_required_and_the_length_must_be_right(self):
        h = headers("plaintext", content_length=None)
        self.assertTrue(fails(web.check_response("plaintext", 200, h, web.BODY["plaintext"])))
        h = headers("plaintext", content_length=None, transfer_encoding="chunked")
        self.assertEqual(web.check_response("plaintext", 200, h, web.BODY["plaintext"]), [])
        h = headers("plaintext", content_length="15")
        self.assertIn("content-length 15", fails(web.check_response("plaintext", 200, h, web.BODY["plaintext"]))[0])

    def test_bodies_must_be_exact(self):
        self.assertTrue(fails(web.check_response("plaintext", 200, headers("plaintext"), b"hello, world!")))
        self.assertTrue(fails(web.check_response("json", 200, headers("json"), b'{"message":"hello"}')))
        self.assertTrue(fails(web.check_response("json", 200, headers("json"), b'{"message":"Hello, World!",')))
        extra = b'{"message":"Hello, World!","x":1}'
        self.assertTrue(fails(web.check_response("json", 200, headers("json", content_length=str(len(extra))), extra)))

    def test_json_whitespace_is_a_warning(self):
        body = b'{"message": "Hello, World!"}'
        problems = web.check_response("json", 200, headers("json", content_length=str(len(body))), body)
        self.assertEqual([level for level, _m in problems], ["warn"])

    def test_status_compression_and_connection_close(self):
        self.assertTrue(fails(web.check_response("json", 500, headers("json"), web.BODY["json"])))
        self.assertTrue(fails(web.check_response("json", 200, headers("json", content_encoding="gzip"),
                                                 web.BODY["json"])))
        problems = web.check_response("json", 200, headers("json", connection="close"), web.BODY["json"])
        self.assertEqual([level for level, _m in problems], ["warn"])

    def test_dates(self):
        self.assertIsNotNone(web.parse_date(DATE))
        self.assertIsNone(web.parse_date("2026-10-22T17:12:19Z"))
        self.assertIsNone(web.parse_date("Thu, 22 Oct 2026 17:12:19 +0000"))
        problems = web.check_response("json", 200, headers("json", date="yesterday"), web.BODY["json"])
        self.assertEqual([level for level, _m in problems], ["warn"])
        self.assertEqual(web.check_date_changes(DATE, "Thu, 22 Oct 2026 17:12:20 GMT"), [])
        self.assertEqual(len(fails(web.check_date_changes(DATE, DATE))), 1)
        self.assertEqual(web.check_date_changes(None, None), [])


OHA = {
    "summary": {"successRate": 1.0, "total": 2.0, "slowest": 0.004, "fastest": 0.00002, "average": 0.00004,
                "requestsPerSec": 1100.0, "totalData": 27000, "sizePerRequest": 27, "sizePerSec": 13500.0},
    "latencyPercentiles": {"p10": 0.00003, "p50": 0.00004, "p99": 0.0005, "p99.9": 0.001},
    "statusCodeDistribution": {"200": 2000, "503": 100},
    "errorDistribution": {"connection closed before message completed": 2},
}


class Oha(unittest.TestCase):
    def test_a_cell_counts_only_200s_and_reports_milliseconds(self):
        cell = web.parse_oha(OHA, "json")
        self.assertEqual(cell["requests"], 2000)
        self.assertAlmostEqual(cell["requests_per_s"], 1000.0)
        self.assertEqual(cell["errors"], 102)
        self.assertAlmostEqual(cell["latency_ms"]["p50"], 0.04)
        self.assertAlmostEqual(cell["latency_ms"]["p99"], 0.5)
        self.assertTrue(cell["size_ok"])
        self.assertFalse(web.parse_oha(OHA, "plaintext")["size_ok"])

    def test_an_empty_run_has_no_rate_and_no_percentiles(self):
        doc = {"summary": {"total": 0.0}, "latencyPercentiles": {}, "statusCodeDistribution": {},
               "errorDistribution": {"connection refused": 5}}
        cell = web.parse_oha(doc, "json")
        self.assertEqual((cell["requests_per_s"], cell["errors"]), (0.0, 5))
        self.assertEqual(cell["latency_ms"], {"p50": None, "p99": None})

    def test_the_command_keeps_http1_keepalive_and_waits_for_ongoing_requests(self):
        cmd = web.oha_command("oha", 4242, "plaintext", 256, 15, threads=2)
        self.assertEqual(cmd[:5], ["oha", "-z", "15s", "-c", "256"])
        for flag in ("-w", "--no-tui"):
            self.assertIn(flag, cmd)
        self.assertEqual(cmd[cmd.index("--http-version") + 1], "1.1")
        self.assertEqual(cmd[cmd.index("--worker-threads") + 1], "2")
        self.assertNotIn("--disable-keepalive", cmd)
        self.assertEqual(cmd[-1], "http://127.0.0.1:4242/plaintext")


class Harness(unittest.TestCase):
    def test_levels(self):
        self.assertEqual(web.parse_levels("16, 256"), [16, 256])
        for bad in ("", "0", "-4"):
            with self.assertRaises(Exception):
                web.parse_levels(bad)

    def test_ports_have_four_digits(self):
        for _ in range(20):
            self.assertTrue(1000 <= web.free_port() <= 9999)

    def test_pipelining_short_of_the_depth_is_a_warning(self):
        self.assertEqual(web.check_pipelining(16), [])
        self.assertEqual([level for level, _m in web.check_pipelining(1)], ["warn"])

    def test_the_sakura_grant_names_only_the_chosen_port(self):
        table = json.loads(web.grant(4242)["SAKURA_PERMISSIONS"])
        self.assertEqual(table["net"], ["127.0.0.1:4242"])
        self.assertEqual(table["env"], [])

    def test_lanes(self):
        single = [n for n, s in web.SERVERS.items() if s[0] == "single"]
        self.assertIn("sakura-node", single)
        self.assertEqual(web.SERVERS["go"][5](1), {"GOMAXPROCS": "1"})
        self.assertEqual(web.SERVERS["go-mt"][5](1), {})
        self.assertEqual(web.SERVERS["rust"][4]("main", 1)[-1], "single")
        self.assertEqual(web.SERVERS["rust-mt"][4]("main", 1)[-1], "multi")
        self.assertEqual({s[0] for s in web.SERVERS.values()}, {"single", "multi"})

    def test_the_table_lists_absent_and_failed_servers(self):
        cell = {"server": "x", "lane": "single", "endpoint": "json", "connections": 16, "requests_per_s": 1234.5,
                "latency_ms": {"p50": 0.1, "p99": None}, "errors": 0}
        text = web.table([cell], [{"server": "sakura-native", "reason": "build failed\nmore"}],
                         [{"server": "y", "endpoint": "plaintext", "problems": [("fail", "no date")]}])
        self.assertIn("| x | single | json | 16 | 1,234 | 0.100 | – | 0 |", text)
        self.assertIn("absent: build failed |", text)
        self.assertIn("failed: no date", text)

    def test_stopping_a_server_stops_its_children(self):
        with tempfile.TemporaryDirectory() as tmp:
            pidfile = Path(tmp) / "child"
            s = web.Server("t", ["sh", "-c", f"sleep 60 & echo $! > {pidfile}; wait"], {}, Path(tmp) / "log")
            deadline = time.monotonic() + 10
            while not (pidfile.exists() and pidfile.read_text().strip()) and time.monotonic() < deadline:
                time.sleep(0.02)
            child = int(pidfile.read_text())
            s.stop()
            for _ in range(100):
                try:
                    os.kill(child, 0)
                except ProcessLookupError:
                    break
                time.sleep(0.02)
            else:
                os.kill(child, signal.SIGKILL)
                self.fail("the server's child outlived stop()")


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = "stub"

    def do_GET(self):  # noqa: N802
        endpoint = self.path.strip("/")
        body = web.BODY.get(endpoint, b"not found")
        self.send_response(200 if endpoint in web.BODY else 404)  # adds Server and a live Date
        self.send_header("content-type", "application/json" if endpoint == "json" else "text/plain")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_a):
        pass


class EndToEnd(unittest.TestCase):
    def test_a_conforming_server_passes_both_endpoints(self):
        srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        t = threading.Thread(target=srv.serve_forever, daemon=True)
        t.start()
        try:
            port = srv.server_address[1]
            self.assertEqual(web.check_server(port), {"json": [], "plaintext": []})
            self.assertEqual(web.pipelined_answers(port), web.PIPELINE_DEPTH)
        finally:
            srv.shutdown()
            srv.server_close()


if __name__ == "__main__":
    unittest.main()
