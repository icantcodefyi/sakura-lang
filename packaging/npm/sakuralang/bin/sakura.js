#!/usr/bin/env node
"use strict";
// Runs `sakura` from this platform's companion package (design §17.147). npm installs only the
// optional dependency whose `os`/`cpu`/`libc` match, so nothing is downloaded here.
const { spawnSync } = require("node:child_process");
const { optionalDependencies } = require("../package.json");

function fail(message) {
  console.error(`sakuralang: ${message}`);
  process.exit(1);
}

const { platform, arch } = process;
let suffix = "";
if (platform === "linux") {
  if (!process.report.getReport().header.glibcVersionRuntime) {
    fail("only glibc Linux is supported; this host has no glibc (musl/Alpine is not supported)");
  }
  suffix = "-gnu";
}
const name = `sakuralang-${platform}-${arch}${suffix}`;
if (!Object.hasOwn(optionalDependencies, name)) {
  const supported = Object.keys(optionalDependencies).join(", ");
  fail(`no build for ${platform}-${arch}${suffix}; supported packages: ${supported}`);
}

let binary;
try {
  binary = require.resolve(`${name}/bin/sakura`);
} catch {
  fail(
    `the platform package ${name} is not installed. Optional dependencies were probably ` +
      `omitted (--omit=optional or --no-optional); reinstall sakuralang without that flag.`,
  );
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
if (result.error) fail(`could not run ${binary}: ${result.error.message}`);
if (result.signal) process.kill(process.pid, result.signal);
process.exit(result.status ?? 1);
