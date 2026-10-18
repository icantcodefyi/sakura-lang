type User = { id: number; name: string; age: number };
// JSON.parse, then the refinements checked by hand, then JSON.stringify of the valid record.
const utf8 = new TextEncoder();
const byteLen = (s: string) => utf8.encode(s).length;
function line(i: number) {
  const name = i % 17 === 0 ? "" : `user${i}`;
  const age = i % 13 === 0 ? 200 : i % 90;
  return `{"id": ${i}, "name": "${name}", "age": ${age}}`;
}
function decode(text: string): User | null {
  let v: unknown;
  try {
    v = JSON.parse(text);
  } catch {
    return null;
  }
  if (v === null || typeof v !== "object" || Array.isArray(v) || !("id" in v) || !("name" in v) || !("age" in v)) return null;
  const { id, name, age } = v;
  if (typeof id !== "number" || !Number.isSafeInteger(id)) return null;
  if (typeof name !== "string" || byteLen(name) === 0 || byteLen(name) > 32) return null;
  if (typeof age !== "number" || !Number.isSafeInteger(age) || age < 0 || age > 150) return null;
  return { id, name, age };
}
export function run(n: number) {
  const lines = [];
  for (let i = 0; i < n; i++) lines.push(line(i));
  const text = lines.join("\n");
  let valid = 0, bytes = 0, ids = 0, invalid = 0;
  for (const l of text.split("\n")) {
    if (l === "") continue;
    const u = decode(l);
    if (u === null) {
      invalid++;
    } else {
      valid++;
      bytes += byteLen(JSON.stringify(u));
      ids += u.id;
    }
  }
  return valid + bytes + ids + 1000 * invalid;
}

import { main } from "../../_shared/typescript/runner.ts";
if (import.meta.main) await main(run, 30000);
