// The playground's compiler, in a worker so the page stays responsive: the Sakura Lang front end
// and JS code generation built for wasm32 (compiler/crates/sakura_playground, built by
// scripts/playground-wasm.sh). Messages in: `{ id, source, emit }`; out: `{ type: "ready" }`
// once, then `{ id, result, ms }` (the crate's JSON answer) or `{ id, error }`.

const wasm = WebAssembly.compileStreaming(fetch("/play/sakura_playground.wasm")).catch(() =>
  fetch("/play/sakura_playground.wasm")
    .then((r) => r.arrayBuffer())
    .then((b) => WebAssembly.compile(b)),
);
let instance = null;

async function exports() {
  instance ??= (await WebAssembly.instantiate(await wasm, {})).exports;
  return instance;
}

function compile(x, source, emit) {
  const input = new TextEncoder().encode(source);
  // `buffer` may grow the memory, which detaches the old `memory.buffer`: read it after.
  const at = x.buffer(input.length);
  new Uint8Array(x.memory.buffer, at, input.length).set(input);
  const n = x.compile_buffer(input.length, emit ? 1 : 0);
  return JSON.parse(new TextDecoder().decode(new Uint8Array(x.memory.buffer, x.buffer_ptr(), n)));
}

exports().then(
  () => postMessage({ type: "ready" }),
  (e) => postMessage({ type: "failed", error: String(e) }),
);

onmessage = async ({ data: { id, source, emit } }) => {
  try {
    const x = await exports();
    const started = performance.now();
    const result = compile(x, source, emit);
    postMessage({ id, result, ms: Math.round(performance.now() - started) });
  } catch (e) {
    // A trap leaves the instance's state unknown: the next request gets a fresh one.
    instance = null;
    postMessage({ id, error: `internal compiler error: ${e?.message ?? e}` });
  }
};
