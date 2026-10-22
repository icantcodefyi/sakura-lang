// TechEmpower-style JSON and plaintext server on Bun.serve (bench/web/README.md).
// AI-assisted (Claude), reviewed by the contributor.
//
// `bun server.mjs <port>` (or PORT) serves on 127.0.0.1 until the process stops. One process,
// one JS thread: the single-threaded lane. `/json` serializes a fresh object per request;
// Bun adds `date` and `content-length`.

const port = Number(process.argv[2] ?? process.env.PORT ?? 8080);

Bun.serve({
  hostname: "127.0.0.1",
  port,
  routes: {
    "/json": () =>
      new Response(JSON.stringify({ message: "Hello, World!" }), {
        headers: { "content-type": "application/json", server: "bun" },
      }),
    "/plaintext": () =>
      new Response("Hello, World!", { headers: { "content-type": "text/plain", server: "bun" } }),
  },
  fetch: () => new Response("not found", { status: 404 }),
});
