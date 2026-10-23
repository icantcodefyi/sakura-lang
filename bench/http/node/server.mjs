// The social app of bench/http/README.md on node:http and node:sqlite (in memory).
// `node server.mjs <port>` serves on 127.0.0.1:<port>, one process. AI-assisted (Claude).
import http from "node:http";
import { DatabaseSync } from "node:sqlite";

const port = Number(process.argv[2] ?? 8080);
const MAX_BODY = 4096;

const db = new DatabaseSync(":memory:");
db.exec(`
  create table users (id integer primary key, name text not null);
  create table posts (id integer primary key, user_id integer not null, body text not null);
  create table likes (post_id integer not null, user_id integer not null, primary key (post_id, user_id));
`);

const insertUser = db.prepare("insert into users (name) values (?) returning id, name");
const insertPost = db.prepare(
  "insert into posts (user_id, body) select id, ? from users where id = ? returning id, user_id, body, 0 as likes",
);
const selectPost = db.prepare(
  "select id, user_id, body, (select count(*) from likes where post_id = posts.id) as likes from posts where id = ?",
);
const insertLike = db.prepare(
  "insert or ignore into likes (post_id, user_id) select posts.id, users.id from posts, users where posts.id = ? and users.id = ?",
);
const countLikes = db.prepare(
  "select id as post_id, (select count(*) from likes where post_id = posts.id) as likes from posts where id = ?",
);
const selectFeed = db.prepare(
  "select id, user_id, body, (select count(*) from likes where post_id = posts.id) as likes from posts order by id desc limit ?",
);

function send(res, status, type, body) {
  res.writeHead(status, { "content-type": type, "content-length": Buffer.byteLength(body) });
  res.end(body);
}
const text = (res, status, body) => send(res, status, "text/plain", body);
const json = (res, status, value) => send(res, status, "application/json", JSON.stringify(value));

/** The body as a JSON object, or undefined after answering 400. */
async function body(req, res) {
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > MAX_BODY) return text(res, 400, "body too large");
    chunks.push(chunk);
  }
  try {
    const value = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    if (value !== null && typeof value === "object") return value;
    text(res, 400, "not an object");
  } catch {
    text(res, 400, "malformed JSON");
  }
}

const isId = (v) => Number.isSafeInteger(v);
const isText = (v, max) => typeof v === "string" && v.length >= 1 && v.length <= max;

async function route(req, res) {
  const url = new URL(req.url, "http://localhost");
  const path = url.pathname;
  const m = /^\/posts\/([^/]+)(\/like)?$/.exec(path);
  if (req.method === "GET" && path === "/health") return text(res, 200, "ok");
  if (req.method === "POST" && path === "/users") {
    const input = await body(req, res);
    if (!input) return;
    if (!isText(input.name, 64)) return text(res, 400, "name: 1 to 64 characters");
    return json(res, 201, insertUser.get(input.name));
  }
  if (req.method === "POST" && path === "/posts") {
    const input = await body(req, res);
    if (!input) return;
    if (!isId(input.user_id) || !isText(input.body, 280)) return text(res, 400, "user_id and body: 1 to 280 characters");
    const post = insertPost.get(input.body, input.user_id);
    return post ? json(res, 201, post) : text(res, 404, "no such user");
  }
  if (m) {
    const id = /^\d+$/.test(m[1]) ? Number(m[1]) : 0;
    if (req.method === "GET" && !m[2]) {
      const post = selectPost.get(id);
      return post ? json(res, 200, post) : text(res, 404, "no such post");
    }
    if (req.method === "POST" && m[2]) {
      if (id === 0) return text(res, 404, "no such post");
      const input = await body(req, res);
      if (!input) return;
      if (!isId(input.user_id)) return text(res, 400, "user_id");
      insertLike.run(id, input.user_id);
      const likes = countLikes.get(id);
      return likes ? json(res, 200, likes) : text(res, 404, "no such post");
    }
  }
  if (req.method === "GET" && path === "/feed") {
    const raw = url.searchParams.get("limit");
    const n = raw !== null && /^-?\d+$/.test(raw) ? Number(raw) : 20;
    return json(res, 200, { posts: selectFeed.all(Math.min(Math.max(n, 1), 100)) });
  }
  text(res, 404, "not found");
}

http
  .createServer((req, res) => {
    route(req, res).catch((e) => text(res, 500, String(e)));
  })
  .listen(port, "127.0.0.1");
