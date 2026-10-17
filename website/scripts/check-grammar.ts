// Smoke check for the Vow grammar: key tokens must get the expected scopes.
import { createHighlighter } from "shiki";
import { vowGrammar } from "../lib/vow/grammar";
import { vowDark, vowLight } from "../lib/vow/themes";

const code = `type Age = Int where 0..=150
fn signup(email: Email, pw: Password) -> User ! {Db, Clock, Fail<SignupError>}
  ensures result.email == email
{
  if Db.exists(users, email) { fail EmailTaken }
  let q = sql"select * from users where id = {id}"
  todo()
}`;

const hl = await createHighlighter({ langs: [vowGrammar], themes: [vowDark, vowLight] });
const lines = hl.codeToTokens(code, { lang: vowGrammar.name as "text", theme: "vow-dark", includeExplanation: true }).tokens;
const scopeOf = (text: string) =>
  lines.flat().find((t) => t.content.trim() === text)?.explanation?.[0]?.scopes.at(-1)?.scopeName;

const expect: Record<string, string> = {
  where: "keyword.other.contract.vow",
  signup: "entity.name.function.vow",
  Clock: "support.type.effect.vow",
  Password: "entity.name.type.vow",
  Fail: "support.type.effect.fail.vow",
  ensures: "keyword.other.contract.vow",
  result: "variable.language.vow",
  fail: "keyword.control.vow",
  exists: "entity.name.function.call.vow",
  sql: "entity.name.function.tagged.vow",
  todo: "keyword.other.hole.vow",
  Age: "entity.name.type.vow",
};
let bad = 0;
for (const [text, scope] of Object.entries(expect)) {
  const got = scopeOf(text);
  if (got !== scope) { bad++; console.error(`✗ ${text}: expected ${scope}, got ${got}`); }
}
console.log(bad ? `${bad} grammar checks failed` : "grammar ok");
process.exit(bad ? 1 : 0);
