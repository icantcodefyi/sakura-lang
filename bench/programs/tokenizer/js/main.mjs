const words = ["alpha", "naïve", "日本語", "🌸", "x1", "Grüße", "ok", "mañana"];
const utf8 = new TextEncoder();
export function run(n) {
  let s = "";
  for (let i = 0; i < n; i++) s = `${s}${words[i % 8]}${i % 5 === 4 ? ", " : " "}`;
  let tokens = 0, bytes = 0, scalars = 0, commas = 0;
  for (const t of s.split(" ")) {
    if (t !== "") {
      tokens++;
      bytes += utf8.encode(t).length;
      scalars += [...t].length;
      if (t.endsWith(",")) commas++;
    }
  }
  return tokens + 3 * bytes + 7 * scalars + 11 * commas;
}
