// Renders the hero's posters, public/hero-light.webp and public/hero-dark.webp: the live scene
// (components/landing/tree-scene.ts) at the poster's size, one frame per theme, on a clear
// background, so the scene fades in over its own picture without a shift. Run it after changing
// the tree: `bun scripts/hero-posters.ts`. Needs Google Chrome (or CHROME=<path>) and cwebp.
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, statSync } from "node:fs";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import { HERO } from "@/lib/hero-tree";

const root = fileURLToPath(new URL("..", import.meta.url));
const chrome = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const { w, h } = HERO.cam.poster;
// Asynchronously, so the page server below keeps answering while Chrome loads from it.
const run = (cmd: string, args: string[]) =>
  new Promise<void>((ok, fail) => {
    const p = spawn(cmd, args, { stdio: "ignore", timeout: 120_000 });
    p.on("error", fail);
    p.on("exit", (code) => (code === 0 ? ok() : fail(new Error(`${cmd} exited with ${code}`))));
  });

const dir = mkdtempSync(join(tmpdir(), "hero-posters-"));
const js = join(dir, "poster.js");
const types: Record<string, string> = { ".js": "text/javascript", ".webp": "image/webp" };
// The page, the bundle, and the bark from public/.
const server = createServer((req, res) => {
  const url = new URL(req.url ?? "/", "http://localhost");
  if (url.pathname === "/") {
    res.setHeader("Content-Type", "text/html");
    res.end(
      `<!doctype html><html class="${url.searchParams.get("theme")}"><body style="margin:0">` +
        `<canvas style="display:block;width:${w}px;height:${h}px"></canvas>` +
        `<script src="/poster.js"></script></body></html>`,
    );
    return;
  }
  const file = url.pathname === "/poster.js" ? js : join(root, "public", normalize(url.pathname));
  try {
    res.setHeader("Content-Type", types[extname(file)] ?? "application/octet-stream");
    res.end(readFileSync(file));
  } catch {
    res.statusCode = 404;
    res.end();
  }
});
try {
  await run("bun", [
    "build",
    join(root, "scripts/hero-poster.client.ts"),
    "--minify",
    "--outfile",
    js,
  ]);
  await new Promise<void>((ok) => server.listen(0, "127.0.0.1", ok));
  const { port } = server.address() as AddressInfo;
  // Both themes at once, each Chrome in its own profile.
  const render = async (theme: string) => {
    const png = join(dir, `${theme}.png`);
    await run(chrome, [
      "--headless=new",
      `--user-data-dir=${join(dir, `profile-${theme}`)}`,
      "--hide-scrollbars",
      "--force-device-scale-factor=1",
      "--default-background-color=00000000",
      "--use-angle=swiftshader",
      "--enable-unsafe-swiftshader",
      `--window-size=${w},${h}`,
      "--virtual-time-budget=20000",
      `--screenshot=${png}`,
      `http://127.0.0.1:${port}/?theme=${theme}`,
    ]);
    const out = join(root, "public", `hero-${theme}.webp`);
    await run("cwebp", ["-quiet", "-q", "72", "-alpha_q", "80", png, "-o", out]);
    console.log(`${out}: ${statSync(out).size} bytes`);
  };
  await Promise.all(["light", "dark"].map(render));
} finally {
  server.close();
  rmSync(dir, { recursive: true, force: true });
}
