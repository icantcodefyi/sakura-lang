import { describe, expect, test } from "bun:test";
import { chmodSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { findExecutable, projectRoot, titleWithImpact } from "../src/lib";

const tmp = mkdtempSync(join(tmpdir(), "sakura-vscode-"));
const file = (path: string, text = "", mode = 0o644) => {
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, text);
  chmodSync(path, mode);
  return path;
};

describe("projectRoot", () => {
  const app = join(tmp, "ws", "app");
  file(join(app, "sakura.toml"));
  file(join(app, "src", "deep", "main.sak"));
  file(join(tmp, "ws", "loose", "x.sak"));

  test("is the nearest sakura.toml, not the workspace folder", () => {
    expect(projectRoot(join(app, "src", "deep", "main.sak"))).toBe(app);
    expect(projectRoot(join(app, "sakura.toml"))).toBe(app);
  });

  test("a nested project wins over its parent", () => {
    const inner = join(app, "vendor", "inner");
    file(join(inner, "sakura.toml"));
    expect(projectRoot(join(inner, "src", "lib.sak"))).toBe(inner);
  });

  test("is undefined outside any project", () => {
    expect(projectRoot(join(tmp, "ws", "loose", "x.sak"))).toBeUndefined();
  });
});

describe("findExecutable", () => {
  const bin = join(tmp, "bin");
  const sakura = file(join(bin, "sakura"), "#!/bin/sh\n", 0o755);
  file(join(tmp, "plain", "sakura"), "", 0o644);

  test("the setting wins, relative to the base directory", () => {
    expect(findExecutable("bin/sakura", { PATH: "" }, tmp)).toEqual({ path: sakura });
    expect(findExecutable(sakura, { PATH: "" }, "/")).toEqual({ path: sakura });
  });

  test("a setting that is not an executable is an error, PATH is not consulted", () => {
    const r = findExecutable("nope/sakura", { PATH: bin }, tmp);
    expect(r).toHaveProperty("error");
    expect((r as { error: string }).error).toContain(join(tmp, "nope", "sakura"));
  });

  test("PATH is searched in order, skipping non-executables and relative entries", () => {
    const executable = process.platform === "win32" ? file(join(bin, "sakura.exe")) : sakura;
    const PATH = ["bin", join(tmp, "plain"), join(tmp, "missing"), bin].join(delimiter);
    expect(findExecutable("", { PATH }, tmp)).toEqual({ path: executable });
  });

  test("Windows skips wrapper directories and uses its semicolon PATH delimiter", () => {
    const wrappers = join(tmp, "windows-wrappers");
    file(join(wrappers, "sakura.cmd"));
    file(join(wrappers, "sakura.bat"));
    file(join(wrappers, "sakura"));
    const native = join(tmp, "windows-native");
    const exe = file(join(native, "sakura.exe"));
    file(join(native, "sakura.cmd"));
    const PATH = [wrappers, native].join(";");
    expect(findExecutable("", { PATH }, tmp, "win32")).toEqual({ path: exe });
    expect(findExecutable("", { Path: PATH }, tmp, "win32")).toEqual({ path: exe });
    expect(findExecutable("", { PATH: wrappers }, tmp, "win32")).toHaveProperty("error");
  });

  test("Windows refuses configured command wrappers without falling back to PATH", () => {
    const native = join(tmp, "windows-configured");
    const exe = file(join(native, "sakura.exe"));
    expect(findExecutable(exe, { PATH: "" }, tmp, "win32")).toEqual({ path: exe });
    for (const name of ["sakura.cmd", "sakura.CMD", "sakura.bat", "sakura.BAT"]) {
      const wrapper = file(join(tmp, "configured-wrappers", name));
      const result = findExecutable(wrapper, { PATH: native }, tmp, "win32");
      expect(result).toHaveProperty("error");
      if ("error" in result) expect(result.error).toContain("sakura.exe");
    }
  });

  test("missing everywhere is an error naming the setting", () => {
    const r = findExecutable("  ", { PATH: join(tmp, "missing") });
    expect((r as { error: string }).error).toContain("sakura.path");
  });
});

test("titleWithImpact shows the impact class", () => {
  expect(titleWithImpact("Use `List`", { impact: "local" })).toBe("Use `List` (impact: local)");
  expect(titleWithImpact("Widen", { impact: "contract_change" })).toBe(
    "Widen (impact: contract change)",
  );
  expect(titleWithImpact("Plain", undefined)).toBe("Plain");
});
