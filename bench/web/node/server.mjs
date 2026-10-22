// TechEmpower-style JSON and plaintext server on node:http (bench/web/README.md).
// AI-assisted (Claude), reviewed by the contributor.
//
// `node server.mjs <port>` (or PORT) serves on 127.0.0.1 until the process stops. One process,
// no cluster: the single-threaded lane. `/json` serializes a fresh object per request; Node adds
// `date`.

import http from "node:http";

const port = Number(process.argv[2] ?? process.env.PORT ?? 8080);

http
  .createServer((req, res) => {
    if (req.url === "/json") {
      const body = JSON.stringify({ message: "Hello, World!" });
      res.writeHead(200, {
        "content-type": "application/json",
        "content-length": Buffer.byteLength(body),
        server: "node",
      });
      res.end(body);
    } else if (req.url === "/plaintext") {
      res.writeHead(200, { "content-type": "text/plain", "content-length": 13, server: "node" });
      res.end("Hello, World!");
    } else {
      res.writeHead(404, { "content-length": 9 });
      res.end("not found");
    }
  })
  .listen(port, "127.0.0.1");
