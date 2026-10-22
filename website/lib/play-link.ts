/**
 * Playground links carry the program in the URL hash: `/play#code=<base64url of raw DEFLATE>`.
 * The browser side uses the native (De)CompressionStream; source.config.ts makes the docs'
 * links with node:zlib's `deflateRawSync`, which reads back the same way.
 */
export const CODE_PREFIX = "#code=";

async function pipe(bytes: Uint8Array, stream: CompressionStream | DecompressionStream) {
  const out = new Blob([bytes as BlobPart]).stream().pipeThrough(stream);
  return new Uint8Array(await new Response(out).arrayBuffer());
}

export async function encodeCode(code: string): Promise<string> {
  const packed = await pipe(new TextEncoder().encode(code), new CompressionStream("deflate-raw"));
  let binary = "";
  for (const b of packed) binary += String.fromCharCode(b);
  return CODE_PREFIX + btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}

/** The program in `hash`, or `null` when it holds none or does not decode. */
export async function decodeCode(hash: string): Promise<string | null> {
  if (!hash.startsWith(CODE_PREFIX)) return null;
  try {
    const b64 = hash.slice(CODE_PREFIX.length).replaceAll("-", "+").replaceAll("_", "/");
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    const text = await pipe(bytes, new DecompressionStream("deflate-raw"));
    return new TextDecoder("utf-8", { fatal: true }).decode(text);
  } catch {
    return null;
  }
}
