// A WebSocket server for tests (RFC 6455 framing by hand, server side), on 127.0.0.1 and an
// ephemeral port. `listen(onOpen)` calls `onOpen(conn, request)` for each upgraded connection;
// `conn` reads the client's frames (`next()`, unmasked) and writes frames of its own.
// `node ws_peer.mjs --serve` serves the echo peer below and prints its port on one line (the
// CLI's JS/WASM test uses it); `node ws_peer.mjs --drive <port>` is the client side of the
// served-upgrade test (`drive`); `node --test` loads this file and finds no tests in it.

import { createHash } from "node:crypto";
import { createServer } from "node:http";
import { connect as netConnect } from "node:net";

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

/** A client frame, masked (RFC 6455 §5.3) unless `masked` is false. */
export function clientFrame(op, payload = Buffer.alloc(0), { fin = true, masked = true } = {}) {
  const f = frame(op, payload, fin);
  if (!masked) return f;
  const at = f.length - Buffer.from(payload).length;
  const mask = Buffer.from([1, 2, 3, 4]);
  const body = Buffer.from(f.subarray(at));
  for (let i = 0; i < body.length; i++) body[i] ^= mask[i & 3];
  const head = Buffer.from(f.subarray(0, at));
  head[1] |= 0x80;
  return Buffer.concat([head, mask, body]);
}

/**
 * A raw HTTP/1.1 request on a fresh TCP connection to `port`: `{ status, headers, body, conn }`
 * once the response head (and, for a non-101, the body to the end) has arrived; `conn` reads the
 * server's frames after a 101.
 */
export function raw(port, path, headers) {
  return new Promise((resolve, reject) => {
    const socket = netConnect(port, "127.0.0.1");
    socket.on("error", reject);
    let buf = Buffer.alloc(0);
    const lines = Object.entries({ host: `127.0.0.1:${port}`, ...headers }).map(([k, v]) => `${k}: ${v}\r\n`);
    socket.write(`GET ${path} HTTP/1.1\r\n${lines.join("")}\r\n`);
    const onData = (chunk) => {
      buf = Buffer.concat([buf, chunk]);
      const end = buf.indexOf("\r\n\r\n");
      if (end < 0) return;
      const [statusLine, ...rest] = buf.subarray(0, end).toString("latin1").split("\r\n");
      const status = Number(statusLine.split(" ")[1]);
      const hs = Object.fromEntries(rest.map((l) => [l.slice(0, l.indexOf(":")).toLowerCase(), l.slice(l.indexOf(":") + 1).trim()]));
      const after = buf.subarray(end + 4);
      if (status === 101) {
        socket.off("data", onData);
        const conn = connection(socket);
        if (after.length > 0) socket.emit("data", after);
        resolve({ status, headers: hs, body: "", conn });
        return;
      }
      if (hs["content-length"] !== undefined && after.length >= Number(hs["content-length"])) {
        socket.off("data", onData);
        socket.end();
        resolve({ status, headers: hs, body: after.toString(), conn: null });
      }
    };
    socket.on("data", onData);
  });
}

const HANDSHAKE = {
  upgrade: "websocket",
  connection: "Upgrade",
  "sec-websocket-key": "dGhlIHNhbXBsZSBub25jZQ==",
  "sec-websocket-version": "13",
};

/**
 * Drives the served program of `tests/wasm.rs::websocket_accept_on_wasm_as_on_js` on `port`
 * with Node's global `WebSocket` and raw connections, printing one line per observation, so the
 * JS and WASM runs can be compared line for line.
 */
async function drive(port) {
  const out = (...xs) => console.log(xs.join(" "));
  const pause = (ms) => new Promise((r) => setTimeout(r, ms));
  const opened = (ws) => new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = () => reject(new Error("the WebSocket failed"));
  });
  const nextMessage = (ws) => new Promise((resolve) => (ws.onmessage = (e) => resolve(e.data)));
  const closedEvent = (ws) => new Promise((resolve) => (ws.onclose = (e) => resolve(e)));
  const text = async (d) => (typeof d === "string" ? d : `bytes ${[...new Uint8Array(await d.arrayBuffer())]}`);

  // Node's own client: subprotocol, no extension, text and binary echoed, the server's close.
  const ws = new WebSocket(`ws://127.0.0.1:${port}/echo`, ["chat"]);
  await opened(ws);
  out("global protocol", ws.protocol, "extensions", JSON.stringify(ws.extensions));
  ws.send("hi");
  out("global echo", await text(await nextMessage(ws)));
  ws.send(new Uint8Array([1, 2]));
  out("global echo", await text(await nextMessage(ws)));
  const closing = closedEvent(ws);
  ws.send("bye");
  const c = await closing;
  out("global close", c.code, c.reason, c.wasClean);
  // The client closes: 1000 back from the server.
  const ws2 = new WebSocket(`ws://127.0.0.1:${port}/echo`);
  await opened(ws2);
  const closing2 = closedEvent(ws2);
  ws2.close(1000, "client done");
  const c2 = await closing2;
  out("global client close", c2.code, c2.wasClean);

  // Refusals: a non-WebSocket upgrade and a plain GET get the handler's answers.
  const h2c = await raw(port, "/plain", { upgrade: "h2c", connection: "Upgrade" });
  out("h2c", h2c.status, h2c.headers.connection, h2c.body);
  const plain = await raw(port, "/echo", {});
  out("plain", plain.status, plain.headers["sec-websocket-version"], plain.body);
  const v8 = await raw(port, "/echo", { ...HANDSHAKE, "sec-websocket-version": "8" });
  out("version 8", v8.status, v8.headers["sec-websocket-version"], v8.headers.connection, v8.body);
  const evil = await raw(port, "/echo", { ...HANDSHAKE, origin: "http://evil.test" });
  out("evil origin", evil.status, evil.headers.connection, evil.body);

  // An admitted browser origin; an extension offer is not echoed; `Upgrade.headers` ride on the 101.
  const ok = await raw(port, "/echo", {
    ...HANDSHAKE,
    origin: "http://OK.test",
    "sec-websocket-protocol": "x, chat",
    "sec-websocket-extensions": "permessage-deflate",
  });
  const h = ok.headers;
  out("ok origin", ok.status, h["sec-websocket-accept"], h["sec-websocket-protocol"], h["sec-websocket-extensions"] ?? "-", h["x-served"]);
  // Fragmented text joined, a ping answered, server frames unmasked.
  ok.conn.write(Buffer.concat([clientFrame(1, "he", { fin: false }), clientFrame(9, "p"), clientFrame(0, "llo")]));
  for (const f of [await ok.conn.next(), await ok.conn.next()]) out("frame", f.op, f.masked, f.payload.toString());
  // `body` returns: 1001.
  ok.conn.write(clientFrame(1, "leave"));
  const gone = await ok.conn.next();
  out("leave", gone.op, gone.payload.readUInt16BE());

  // The wire's failures, each closing the connection with its code.
  for (const [what, bytes] of [
    ["unmasked", clientFrame(1, "x", { masked: false })],
    ["too large", clientFrame(2, Buffer.alloc(17))],
    ["bad utf-8", clientFrame(1, Buffer.from([0xc3]))],
  ]) {
    const r = await raw(port, "/echo", HANDSHAKE);
    r.conn.write(bytes);
    const f = await r.conn.next();
    out(what, f.op, f.payload.readUInt16BE());
  }
  await pause(200);

  // Topics: a message from one member reaches the other, not itself; the third socket is over
  // the limit of 2 (`SAKURA_MAX_SOCKETS`) and is answered 503.
  const a = new WebSocket(`ws://127.0.0.1:${port}/room`);
  const b = new WebSocket(`ws://127.0.0.1:${port}/room`);
  const [ja, jb] = [nextMessage(a), nextMessage(b)];
  await Promise.all([opened(a), opened(b)]);
  out("joined", await ja, await jb);
  const third = await raw(port, "/room", HANDSHAKE);
  out("limit", third.status, third.headers["retry-after"], third.body);
  const [ga, gb] = [nextMessage(a), nextMessage(b)];
  a.send("from a");
  out("room", await ga, await gb);
  a.close();
  b.close();
  await pause(100);
}

if (process.argv[2] === "--serve") {
  const { port } = await listen(echo);
  console.log(port);
} else if (process.argv[2] === "--drive") {
  await drive(Number(process.argv[3]));
  process.exit(0);
}
