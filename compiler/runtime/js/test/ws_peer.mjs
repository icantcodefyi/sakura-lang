// A WebSocket server for tests (RFC 6455 framing by hand, server side), on 127.0.0.1 and an
// ephemeral port. `listen(onOpen)` calls `onOpen(conn, request)` for each upgraded connection;
// `conn` reads the client's frames (`next()`, unmasked) and writes frames of its own.
// `node ws_peer.mjs --serve` serves the echo peer below and prints its port on one line (the
// CLI's JS/WASM test uses it); `node --test` loads this file and finds no tests in it.

import { createHash } from "node:crypto";
import { createServer } from "node:http";

const GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/** An unmasked server frame. */
export function frame(op, payload = Buffer.alloc(0), fin = true) {
  const p = Buffer.from(payload);
  const n = p.length;
  const head = n < 126 ? Buffer.from([0, n]) : n < 65536 ? Buffer.from([0, 126, n >> 8, n & 255]) : Buffer.alloc(10);
  if (n >= 65536) {
    head[1] = 127;
    head.writeBigUInt64BE(BigInt(n), 2);
  }
  head[0] = (fin ? 0x80 : 0) | op;
  return Buffer.concat([head, p]);
}

export const closeFrame = (code, reason = "") => {
  const p = Buffer.alloc(2 + Buffer.byteLength(reason));
  p.writeUInt16BE(code);
  p.write(reason, 2);
  return frame(8, p);
};

/** Serves upgrades on 127.0.0.1; `answer(req)` may return a status to refuse one with. */
export function listen(onOpen, answer = () => 101) {
  const server = createServer((_req, res) => res.writeHead(404).end());
  const upgraded = new Set();
  server.on("upgrade", (req, socket) => {
    upgraded.add(socket);
    const status = answer(req);
    if (status !== 101) {
      socket.end(`HTTP/1.1 ${status} No\r\nlocation: ws://127.0.0.1:1/\r\ncontent-length: 0\r\n\r\n`);
      return;
    }
    const accept = createHash("sha1").update(req.headers["sec-websocket-key"] + GUID).digest("base64");
    const proto = req.headers["sec-websocket-protocol"]?.split(",")[0].trim();
    const extra = proto ? `sec-websocket-protocol: ${proto}\r\n` : "";
    socket.write(`HTTP/1.1 101 Switching Protocols\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-accept: ${accept}\r\n${extra}\r\n`);
    onOpen(connection(socket), req);
  });
  return new Promise((resolve) =>
    server.listen(0, "127.0.0.1", () =>
      resolve({
        port: server.address().port,
        close: () => {
          for (const s of upgraded) s.destroy();
          server.closeAllConnections();
          server.close();
        },
      }),
    ),
  );
}

/** The client's frames, unmasked, as `{ op, fin, payload }`, read with `next()`. */
function connection(socket) {
  let buf = Buffer.alloc(0);
  const frames = [];
  let wake = null;
  socket.on("data", (chunk) => {
    buf = Buffer.concat([buf, chunk]);
    for (;;) {
      if (buf.length < 2) break;
      let len = buf[1] & 0x7f;
      let at = 2;
      if (len === 126) {
        if (buf.length < 4) break;
        len = buf.readUInt16BE(2);
        at = 4;
      } else if (len === 127) {
        if (buf.length < 10) break;
        len = Number(buf.readBigUInt64BE(2));
        at = 10;
      }
      const masked = (buf[1] & 0x80) !== 0;
      const mask = masked ? buf.subarray(at, at + 4) : null;
      at += masked ? 4 : 0;
      if (buf.length < at + len) break;
      const payload = Buffer.from(buf.subarray(at, at + len));
      if (mask) for (let i = 0; i < len; i++) payload[i] ^= mask[i & 3];
      frames.push({ op: buf[0] & 0x0f, fin: (buf[0] & 0x80) !== 0, masked, payload });
      buf = buf.subarray(at + len);
    }
    wake?.();
  });
  const ended = new Promise((resolve) => socket.on("close", resolve));
  socket.on("error", () => {});
  return {
    socket,
    ended,
    write: (b) => socket.write(b),
    /** The next client frame, or `null` once the connection ended. */
    async next() {
      while (frames.length === 0) {
        if (socket.destroyed || socket.readableEnded) return null;
        await Promise.race([new Promise((r) => (wake = r)), ended]);
      }
      return frames.shift();
    },
  };
}

/** The echo peer: each data frame back as it came; a close answered with its own code. */
export function echo(conn) {
  (async () => {
    for (;;) {
      const f = await conn.next();
      if (f === null) return;
      if (f.op === 8) {
        conn.write(frame(8, f.payload));
        conn.socket.end();
        return;
      }
      if (f.op === 1 || f.op === 2) conn.write(frame(f.op, f.payload));
    }
  })();
}

if (process.argv[2] === "--serve") {
  const { port } = await listen(echo);
  console.log(port);
}
