// The host's WebSocket client (RFC 0002 §1, §6; plan `web-apis.md` phase 8) against a live
// local peer (`ws_peer.mjs`): text, binary and fragmented messages, ping/pong, close codes 1000,
// 1001 (scope exit), 1002, 1007, 1009, the 5-second close timeout, the queue (64 messages or
// `max_message` bytes) that stops the client reading, a pong only while the socket needs no
// drain, no redirect on the upgrade, and `net`. The scripted handler and the
// `std/websocket` rules are the `websocket_*` semantics fixtures; the same program against this
// peer on JS and WASM is `tests/wasm.rs::websocket_host_on_wasm_as_on_js`. A file of its own:
// `SAKURA_PERMISSIONS` is read once per process, so it is set before the first connect.

import assert from "node:assert/strict";
import net from "node:net";
import { after, test } from "node:test";
import * as rt from "../sakura_runtime.js";
import { closeFrame, echo, frame, listen } from "./ws_peer.mjs";

class Message extends rt.Variant {}
Message.prototype.$t = rt.variantType("Message", "std::websocket", [["Text", [["String"]]], ["Binary", [["Bytes"]]]]);
class CloseInfo {
  constructor(code, reason, clean) {
    Object.assign(this, { code, reason, clean });
  }
}
class Received extends rt.Variant {}
Received.prototype.$t = rt.variantType("Received", "std::websocket", [["Data", [["Message"]]], ["Closed", [["CloseInfo"]]]]);
rt.def("std::websocket::Message", Message);
rt.def("std::websocket::CloseInfo", CloseInfo);
rt.def("std::websocket::Received", Received);

/** Each connection's path picks its behaviour; `/moved` is refused with a 301. */
const behaviours = new Map();
const peer = await listen(
  (conn, req) => (behaviours.get(req.url) ?? echo)(conn, req),
  (req) => (req.url === "/moved" ? 301 : 101),
);
after(() => peer.close());
process.env.SAKURA_PERMISSIONS = JSON.stringify({ net: [`127.0.0.1:${peer.port}`] });

const options = ({ subprotocols = [], headers = [], max = 1 << 24 } = {}) => ({
  subprotocols: rt.list(subprotocols),
  headers: rt.list(headers),
  max_message: max,
});
const run = async (f, args) => {
  const o = await rt.runTask(f, args);
  if (o.kind !== "ok") assert.fail(JSON.stringify(o));
  return o.value;
};
/** Connects to `path` on the peer; the socket id and chosen subprotocol. */
async function connect(path, opts = options()) {
  const r = await run(rt.ws_connect, [`ws://127.0.0.1:${peer.port}${path}`, opts]);
  assert.equal(r.tag, 0, `connect failed: ${r.fields[0]}`);
  return r.fields[0];
}
/**
 * The client's TCP sockets, found by `setNoDelay`, which the host calls on each one it opens: the
 * queue bound is read on the client's side (paused, `bytesRead`), since how much a peer can write
 * before TCP holds it back depends on the OS's buffers (a Windows peer may write it all).
 */
const sockets = [];
const setNoDelay = net.Socket.prototype.setNoDelay;
net.Socket.prototype.setNoDelay = function (...args) {
  sockets.push(this);
  return setNoDelay.apply(this, args);
};
/** The client socket of the latest connection to the peer. */
const clientSocket = () => sockets.findLast((s) => s.remotePort === peer.port);
/** Waits for `cond`, failing with `what` after 10 s (a regression fails instead of hanging). */
const until = async (cond, what) => {
  const deadline = Date.now() + 10000;
  while (!cond()) {
    if (Date.now() > deadline) assert.fail(what);
    await new Promise((r) => setTimeout(r, 5));
  }
};
const receive = async (id) => (await run(rt.ws_receive, [id])).fields[0];
const send = (id, m) => run(rt.ws_send, [id, m]);
const text = (s) => new Message(0, [s]);
const binary = (b) => new Message(1, [Uint8Array.from(b)]);
const data = (m) => new Received(0, [m]);
const closed = (code, reason, clean) => new Received(1, [new CloseInfo(code, reason, clean)]);

test("text and binary echo, a fragmented message is joined, a ping is answered with its pong", async () => {
  let ponged;
  const pong = new Promise((resolve) => (ponged = resolve));
  behaviours.set("/frag", async (conn) => {
    conn.write(frame(1, "he", false));
    conn.write(frame(9, "p1"));
    conn.write(frame(0, "llo"));
    ponged(await conn.next());
    echo(conn);
  });
  const [id, chosen] = await connect("/frag", options({ subprotocols: ["chat"] }));
  assert.equal(chosen, "chat");
  assert.deepEqual(await receive(id), data(text("hello")));
  const p = await pong;
  assert.deepEqual([p.op, p.masked, p.payload.toString()], [10, true, "p1"]);
  assert.deepEqual(await send(id, text("hi")), rt.Ok(true));
  assert.deepEqual(await receive(id), data(text("hi")));
  assert.deepEqual(await send(id, binary([0, 255, 7])), rt.Ok(true));
  assert.deepEqual(await receive(id), data(binary([0, 255, 7])));
  rt.file_close(id);
});

test("close(1000) completes the handshake; the close is returned again and a send is Ok(false)", async () => {
  const [id] = await connect("/echo");
  assert.deepEqual(rt.ws_close(id, 1000, "bye"), rt.Ok(undefined));
  assert.deepEqual(await receive(id), closed(1000, "bye", true));
  assert.deepEqual(await receive(id), closed(1000, "bye", true));
  assert.deepEqual(await send(id, text("late")), rt.Ok(false));
  rt.file_close(id);
});

test("the peer's close is answered with its code and received after the messages before it", async () => {
  let answer;
  behaviours.set("/bye", async (conn) => {
    conn.write(Buffer.concat([frame(1, "last"), closeFrame(1000, "done")]));
    answer = await conn.next();
  });
  const [id] = await connect("/bye");
  assert.deepEqual(await receive(id), data(text("last")));
  assert.deepEqual(await receive(id), closed(1000, "done", true));
  await new Promise((r) => setTimeout(r, 50));
  assert.equal(answer.payload.readUInt16BE(), 1000);
  rt.file_close(id);
});

test("leaving the scope closes an open socket with 1001", async () => {
  let seen;
  const got = new Promise((resolve) => {
    behaviours.set("/leave", async (conn) => {
      seen = await conn.next();
      resolve();
    });
  });
  const [id] = await connect("/leave");
  rt.file_close(id);
  await got;
  assert.deepEqual([seen.op, seen.payload.readUInt16BE()], [8, 1001]);
});

test("a message over max_message fails the connection with 1009 before its payload is held", async () => {
  let seen;
  behaviours.set("/big", async (conn) => {
    conn.write(frame(2, Buffer.alloc(10)));
    seen = await conn.next();
  });
  const [id] = await connect("/big", options({ max: 4 }));
  assert.deepEqual(await receive(id), closed(1009, "message too large", false));
  await new Promise((r) => setTimeout(r, 50));
  assert.deepEqual([seen.op, seen.payload.readUInt16BE()], [8, 1009]);
  rt.file_close(id);
});

test("text that is not UTF-8 fails with 1007, a masked server frame with 1002", async () => {
  behaviours.set("/bad-utf8", (conn) => conn.write(frame(1, Buffer.from([0xc3]))));
  behaviours.set("/masked", (conn) => conn.write(Buffer.from([0x81, 0x80, 0, 0, 0, 0])));
  for (const [path, code, reason] of [
    ["/bad-utf8", 1007, "invalid UTF-8"],
    ["/masked", 1002, "protocol error"],
  ]) {
    const [id] = await connect(path);
    assert.deepEqual(await receive(id), closed(code, reason, false));
    rt.file_close(id);
  }
});

test("at most 64 messages wait: the client stops reading until they are received", async () => {
  const n = 300;
  const frameLen = 10 + 65536;
  let sent;
  const sending = new Promise((resolve) => (sent = resolve));
  behaviours.set("/flood", (c) => {
    for (let i = 0; i < n; i++) c.write(frame(2, Buffer.alloc(65536, i)));
    sent();
  });
  const [id] = await connect("/flood");
  const sock = clientSocket();
  await sending;
  await until(() => sock.isPaused(), "the client never stopped reading");
  // 64 messages, then at most one read chunk and the stream's high-water mark more.
  assert.ok(sock.bytesRead < 80 * frameLen, `read ${sock.bytesRead} bytes while paused`);
  for (let i = 0; i < n; i++) {
    const r = await receive(id);
    assert.equal(r.fields[0].fields[0][0], i & 255);
  }
  rt.file_close(id);
});

test("more than max_message waiting bytes stop the client reading, under 64 messages", async () => {
  const n = 8;
  const size = 768 * 1024;
  behaviours.set("/heavy", (c) => {
    for (let i = 0; i < n; i++) c.write(frame(2, Buffer.alloc(size, i)));
  });
  const [id] = await connect("/heavy", options({ max: 1 << 20 }));
  const sock = clientSocket();
  await until(() => sock.isPaused(), "the client never stopped reading");
  // Two messages (1.5 MiB, over the 1 MiB bound) wait; the third is not read whole.
  assert.ok(sock.bytesRead < 3 * (10 + size), `read ${sock.bytesRead} bytes while paused`);
  for (let i = 0; i < n; i++) {
    const r = await receive(id);
    assert.deepEqual([r.fields[0].fields[0].length, r.fields[0].fields[0][0]], [size, i]);
  }
  rt.file_close(id);
});

test("a ping is answered only while the socket's write buffer needs no drain", async () => {
  let conn;
  const opened = new Promise((resolve) => {
    behaviours.set("/pings", (c) => {
      conn = c;
      resolve();
    });
  });
  const [id] = await connect("/pings");
  const sock = clientSocket();
  await opened;
  // When a real buffer needs draining is the OS's choice; the rule is read from the flag.
  let full = true;
  Object.defineProperty(sock, "writableNeedDrain", { get: () => full, configurable: true });
  conn.write(Buffer.concat([frame(9, "p1"), frame(1, "one")]));
  assert.deepEqual(await receive(id), data(text("one")));
  full = false;
  conn.write(Buffer.concat([frame(9, "p2"), frame(1, "two")]));
  assert.deepEqual(await receive(id), data(text("two")));
  assert.deepEqual(await send(id, text("end")), rt.Ok(true));
  const frames = [await conn.next(), await conn.next()].map((f) => [f.op, f.payload.toString()]);
  assert.deepEqual(frames, [
    [10, "p2"],
    [1, "end"],
  ]);
  rt.file_close(id);
});

test("a close the peer never answers ends after 5 seconds as 1006", async () => {
  behaviours.set("/mute", () => {});
  const [id] = await connect("/mute");
  rt.ws_close(id, 3000, "");
  const started = Date.now();
  assert.deepEqual(await receive(id), closed(1006, "", false));
  assert.ok(Date.now() - started >= 4900);
  rt.file_close(id);
});

test("the upgrade follows no redirect, and a host outside net is refused before connecting", async () => {
  const moved = await run(rt.ws_connect, [`ws://127.0.0.1:${peer.port}/moved`, options()]);
  assert.deepEqual(moved, rt.Err("the handshake answered 301, not 101"));
  for (const [url, host] of [
    ["ws://c.example/", "c.example:80"],
    ["wss://c.example/", "c.example:443"],
  ]) {
    assert.deepEqual(await run(rt.ws_connect, [url, options()]), rt.Err(`${host} is not in the manifest's net list`));
  }
  const own = await run(rt.ws_connect, [`ws://127.0.0.1:${peer.port}/`, options({ headers: [["Sec-WebSocket-Key", "x"]] })]);
  assert.deepEqual(own, rt.Err("header Sec-WebSocket-Key is set by the handshake"));
});

test("headers reach the handshake", async () => {
  let token;
  behaviours.set("/auth", (conn, req) => {
    token = req.headers["x-token"];
    echo(conn);
  });
  const [id] = await connect("/auth", options({ headers: [["x-token", "t1"]] }));
  assert.equal(token, "t1");
  rt.file_close(id);
});

// The server role (RFC 0002 §2.10, design §17.289; plan `web-apis.md` phase 8s). A served
// program against Node's global `WebSocket` and raw clients, on JS and WASM, is
// `tests/wasm.rs::websocket_accept_on_wasm_as_on_js`; these are the runtime's own rules.

test("accept outside the performing task's own upgrade request is refused, and writes nothing", () => {
  const hs = { key: "dGhlIHNhbXBsZSBub25jZQ==", subprotocols: rt.list([]), origin: rt.None };
  const up = { subprotocol: rt.None, origins: rt.list([]), headers: rt.list([]), max_message: 16 };
  assert.deepEqual(rt.ws_accept(hs, up), rt.Err("not a served upgrade request"));
  const conn = { socket: { writable: true }, head: new Uint8Array(0), key: "other", upgraded: false };
  assert.deepEqual(rt.wsAcceptOn(conn, hs, up), rt.Err("not a served upgrade request"));
  const own = { ...conn, key: hs.key };
  const bad = { ...up, headers: rt.list([["Sec-WebSocket-Extensions", "x"]]) };
  assert.deepEqual(rt.wsAcceptOn(own, hs, bad), rt.Err("header Sec-WebSocket-Extensions is set by the handshake"));
  assert.equal(own.upgraded, false);
});

test("publish skips a subscriber whose send buffer holds max_message bytes, or that is closing", () => {
  const s = (writableLength, extra = {}) => ({ closing: false, closed: null, max: 8, socket: { writableLength }, ...extra });
  assert.equal(rt.wsSkips(s(0), 8), false);
  assert.equal(rt.wsSkips(s(7), 8), false);
  assert.equal(rt.wsSkips(s(8), 1), true);
  assert.equal(rt.wsSkips(s(0), 9), true);
  assert.equal(rt.wsSkips(s(0, { closing: true }), 1), true);
});

test("publish writes to every subscriber but except, without waiting; subscriptions end with the socket", async () => {
  const got = [];
  behaviours.set("/topic", async (conn) => got.push(await conn.next()));
  const [a] = await connect("/topic");
  const [b] = await connect("/topic");
  assert.deepEqual(rt.ws_subscribe(a, "t"), rt.Ok(true));
  assert.deepEqual(rt.ws_subscribe(b, "t"), rt.Ok(true));
  assert.deepEqual(rt.ws_publish("t", text("news"), rt.Some(a)), rt.Ok(1));
  await until(() => got.length === 1, "the subscriber never received the message");
  assert.deepEqual([got[0].op, got[0].masked, got[0].payload.toString()], [1, true, "news"]);
  rt.file_close(b);
  assert.deepEqual(rt.ws_publish("t", text("again"), rt.None), rt.Ok(1));
  assert.deepEqual(rt.ws_subscribe(b, "t"), rt.Ok(false));
  rt.file_close(a);
  assert.deepEqual(rt.ws_publish("t", text("none"), rt.None), rt.Ok(0));
});

test("SAKURA_MAX_SOCKETS bounds the open sockets, and an invalid value is a D091", async () => {
  const [id] = await connect("/echo");
  process.env.SAKURA_MAX_SOCKETS = "1";
  try {
    const r = await run(rt.ws_connect, [`ws://127.0.0.1:${peer.port}/echo`, options()]);
    assert.deepEqual(r, rt.Err("the socket limit of 1 is reached"));
    process.env.SAKURA_MAX_SOCKETS = "0";
    const o = await rt.runTask(rt.ws_connect, [`ws://127.0.0.1:${peer.port}/echo`, options()]);
    assert.equal(o.kind, "defect");
    assert.match(o.message, /SAKURA_MAX_SOCKETS must be a positive integer number of sockets/);
  } finally {
    delete process.env.SAKURA_MAX_SOCKETS;
  }
  rt.file_close(id);
  const [again] = await connect("/echo");
  rt.file_close(again);
});
