// The social app of bench/http/README.md on Bun.serve and bun:sqlite (in memory).
// `bun server.mjs <port>` serves on 127.0.0.1:<port>. AI-assisted (Claude).
import { Database } from "bun:sqlite";

const port = Number(process.argv[2] ?? 8080);
const MAX_BODY = 4096;

const db = new Database(":memory:");
db.run("create table users (id integer primary key, name text not null)");
db.run("create table posts (id integer primary key, user_id integer not null, body text not null)");
db.run(
  "create table likes (post_id integer not null, user_id integer not null, primary key (post_id, user_id))",
);

const insertUser = db.query("insert into users (name) values (?) returning id, name");
const insertPost = db.query(
  "insert into posts (user_id, body) select id, ? from users where id = ? returning id, user_id, body, 0 as likes",
);
const selectPost = db.query(
  "select id, user_id, body, (select count(*) from likes where post_id = posts.id) as likes from posts where id = ?",
);
const insertLike = db.query(
  "insert or ignore into likes (post_id, user_id) select posts.id, users.id from posts, users where posts.id = ? and users.id = ?",
);
const countLikes = db.query(
  "select id as post_id, (select count(*) from likes where post_id = posts.id) as likes from posts where id = ?",
);
const selectFeed = db.query(
  "select id, user_id, body, (select count(*) from likes where post_id = posts.id) as likes from posts order by id desc limit ?",
);

const text = (status, body) => new Response(body, { status, headers: { "content-type": "text/plain" } });
const json = (status, value) => Response.json(value, { status });

/** The body as a JSON object, or a 400 response. */
async function body(req) {
  const raw = await req.text();
  if (raw.length > MAX_BODY) return text(400, "body too large");
  try {
    const value = JSON.parse(raw);
    return value !== null && typeof value === "object" ? value : text(400, "not an object");
  } catch {
    return text(400, "malformed JSON");
  }
}

const isId = (v) => Number.isSafeInteger(v);
const isText = (v, max) => typeof v === "string" && v.length >= 1 && v.length <= max;
const postId = (req) => (/^\d+$/.test(req.params.id) ? Number(req.params.id) : 0);

Bun.serve({
  hostname: "127.0.0.1",
  port,
  routes: {
    "/health": { GET: () => text(200, "ok") },
    "/users": {
      POST: async (req) => {
        const input = await body(req);
        if (input instanceof Response) return input;
        if (!isText(input.name, 64)) return text(400, "name: 1 to 64 characters");
        return json(201, insertUser.get(input.name));
      },
    },
    "/posts": {
      POST: async (req) => {
        const input = await body(req);
        if (input instanceof Response) return input;
        if (!isId(input.user_id) || !isText(input.body, 280)) return text(400, "user_id and body: 1 to 280 characters");
        const post = insertPost.get(input.body, input.user_id);
        return post ? json(201, post) : text(404, "no such user");
      },
    },
    "/posts/:id": {
      GET: (req) => {
        const post = selectPost.get(postId(req));
        return post ? json(200, post) : text(404, "no such post");
      },
    },
    "/posts/:id/like": {
      POST: async (req) => {
        const id = postId(req);
        if (id === 0) return text(404, "no such post");
        const input = await body(req);
        if (input instanceof Response) return input;
        if (!isId(input.user_id)) return text(400, "user_id");
        insertLike.run(id, input.user_id);
        const likes = countLikes.get(id);
        return likes ? json(200, likes) : text(404, "no such post");
      },
    },
    "/feed": {
      GET: (req) => {
        const raw = new URL(req.url).searchParams.get("limit");
        const n = raw !== null && /^-?\d+$/.test(raw) ? Number(raw) : 20;
        return json(200, { posts: selectFeed.all(Math.min(Math.max(n, 1), 100)) });
      },
    },
  },
  fetch: () => text(404, "not found"),
});
