// The playground's committed compiler (public/play/sakura_playground.wasm, built by
// scripts/playground-wasm.sh): its bytes match the checksum beside it, it imports nothing, and it
// checks and builds a program. `bun run build` runs this; CI runs it on a fresh build too.
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

const dir = new URL("../public/play/", import.meta.url);
const bytes = readFileSync(new URL("sakura_playground.wasm", dir));
const [expected] = readFileSync(new URL("sakura_playground.wasm.sha256", dir), "utf8").split(/\s/);
const actual = createHash("sha256").update(bytes).digest("hex");
if (actual !== expected) {
  throw new Error(
    `public/play/sakura_playground.wasm is ${actual}, its .sha256 says ${expected}; run scripts/playground-wasm.sh`,
  );
}

const wasm = await WebAssembly.compile(bytes);
const imports = WebAssembly.Module.imports(wasm);
if (imports.length > 0) throw new Error(`the playground wasm imports ${JSON.stringify(imports)}`);
const { exports: x } = await WebAssembly.instantiate(wasm, {});

function compile(source, emit) {
  const input = new TextEncoder().encode(source);
  // `buffer` may grow the memory, which detaches the old `memory.buffer`: read it after.
  const at = x.buffer(input.length);
  new Uint8Array(x.memory.buffer, at, input.length).set(input);
  const n = x.compile_buffer(input.length, emit ? 1 : 0);
  return JSON.parse(new TextDecoder().decode(new Uint8Array(x.memory.buffer, x.buffer_ptr(), n)));
}

const hello = 'pub fn main() -> Unit ! {Console} {\n  Console.print("hello")\n}\n';
const built = compile(hello, true);
if (built.errors || !built.files?.some(([path]) => path === "__main__.js")) {
  throw new Error(`the playground wasm did not build hello:\n${built.human}`);
}
const broken = compile("pub fn main() -> Int { true }\n", false);
if (!broken.errors || !broken.diagnostics[0]?.code) {
  throw new Error(`the playground wasm did not report a type error:\n${broken.human}`);
}
console.log(
  `playground wasm ${actual.slice(0, 12)}: ${bytes.length} bytes, compiler ${built.version}`,
);
