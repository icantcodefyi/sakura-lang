#!/usr/bin/env python3
"""Seed a running bench/http server over HTTP: users, posts by random users, distinct likes.

    python3 bench/http/seed.py http://127.0.0.1:8080 [--users 1000] [--posts 10000] [--likes 20000]

Python 3.11+ standard library only. Every write goes through the app's own endpoints, so a
seeded server holds users 1..users and posts 1..posts on every implementation. Exits 1 if an
answer is not the expected status or id. AI-assisted (Claude).
"""

import argparse
import http.client
import json
import random
import sys
from urllib.parse import urlsplit


class SeedError(Exception):
    """An answer that was not the expected status or id."""


def seed(base, users, posts, likes, rng_seed=20261023):
    u = urlsplit(base)
    conn = http.client.HTTPConnection(u.hostname, u.port, timeout=10)
    rng = random.Random(rng_seed)

    def post(path, body, want):
        conn.request("POST", path, json.dumps(body), {"content-type": "application/json"})
        r = conn.getresponse()
        data = r.read()
        if r.status != want:
            raise SeedError(f"seed: POST {path} answered {r.status}: {data[:200]!r}")
        return json.loads(data)

    for i in range(1, users + 1):
        got = post("/users", {"name": f"user {i}"}, 201)
        if got["id"] != i:
            raise SeedError(f"seed: user {i} got id {got['id']}; seed a fresh server")
    for i in range(1, posts + 1):
        got = post("/posts", {"user_id": rng.randint(1, users), "body": f"seed post {i}"}, 201)
        if got["id"] != i:
            raise SeedError(f"seed: post {i} got id {got['id']}; seed a fresh server")
    pairs = set()
    while len(pairs) < min(likes, users * posts):
        pairs.add((rng.randint(1, posts), rng.randint(1, users)))
    for post_id, user_id in sorted(pairs):
        post(f"/posts/{post_id}/like", {"user_id": user_id}, 200)
    conn.close()
    return {"users": users, "posts": posts, "likes": len(pairs)}


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("base", help="the server, e.g. http://127.0.0.1:8080")
    ap.add_argument("--users", type=int, default=1000)
    ap.add_argument("--posts", type=int, default=10000)
    ap.add_argument("--likes", type=int, default=20000)
    a = ap.parse_args()
    try:
        print(json.dumps(seed(a.base, a.users, a.posts, a.likes)))
    except SeedError as e:
        sys.exit(str(e))


if __name__ == "__main__":
    main()
