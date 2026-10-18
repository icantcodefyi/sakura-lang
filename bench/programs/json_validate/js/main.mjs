// JSON.parse, then the refinements checked by hand, then JSON.stringify of the valid record.
const utf8 = new TextEncoder();
const byteLen = (s) => utf8.encode(s).length;
function line(i) {
  const name = i % 17 === 0 ? "" : `user${i}`;
  const age = i % 13 === 0 ? 200 : i % 90;
  return `{"id": ${i}, "name": "${name}", "age": ${age}}`;
}
function decode(text) {
  let v;
  try {
    v = JSON.parse(text);
  } catch {
    return null;
  }
  if (v === null || typeof v !== "object" || Array.isArray(v)) return null;
  const { id, name, age } = v;
  if (!Number.isSafeInteger(id)) return null;
  if (typeof name !== "string" || byteLen(name) === 0 || byteLen(name) > 32) return null;
  if (!Number.isSafeInteger(age) || age < 0 || age > 150) return null;
  return { id, name, age };
}
export function run(n) {
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
