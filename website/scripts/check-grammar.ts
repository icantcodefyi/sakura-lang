// Smoke check for the Vow grammar against docs/design.md §2–§3: key tokens must get the expected scopes.
import { createHighlighter } from "shiki";
import { vowGrammar } from "../lib/vow/grammar";
import { vowDark, vowLight } from "../lib/vow/themes";

const code = `/// Doc comment
/* outer /* nested */ still comment */
type Percent = Int where it >= 0 && it <= 100
error EmailTaken(email: Email)
fn signup(email: Email, pw: Password) -> User ! {Db, Clock, Fail<EmailTaken | Banned>}
  ensures result.email == email
{
  if Db.exists(users, email) || !ok { fail EmailTaken { email } }
  var mask = 0b1010
  let q = sql"select * from users where id = {Map.get({a: 1}, k)} and x"
  let r = try { parse(s)? } catch { Banned => ?fallback }
  resume
}
fn map<A, B, e>(xs: List<A>, f: fn(A) -> B ! e) -> List<B> ! e {
  with handler Counter { var n = 0; fn next() { n = n + 1; n } } { Task.spawn(s, f) }
}`;

const hl = await createHighlighter({ langs: [vowGrammar], themes: [vowDark, vowLight] });
const tokens = hl
  .codeToTokens(code, {
    lang: vowGrammar.name as "text",
    theme: "vow-dark",
    includeExplanation: true,
  })
  .tokens.flat()
  .flatMap((t) => t.explanation ?? []);
const scopeOf = (text: string, nth = 0) =>
  tokens.filter((t) => t.content.trim() === text)[nth]?.scopes.at(-1)?.scopeName;

const expect: [string, string, number?][] = [
  ["/// Doc comment", "comment.line.documentation.vow"],
  ["still comment", "comment.block.vow"],
  ["where", "keyword.other.contract.vow"],
  ["it", "variable.language.vow"],
  ["&&", "keyword.operator.logical.vow"],
  ["error", "storage.type.vow"],
  ["EmailTaken", "entity.name.type.vow"],
  ["signup", "entity.name.function.vow"],
  ["Clock", "support.type.effect.vow"],
  ["Password", "entity.name.type.vow"],
  ["Fail", "support.type.effect.fail.vow"],
  ["Banned", "entity.name.type.vow"],
  ["ensures", "keyword.other.contract.vow"],
  ["result", "variable.language.vow"],
  ["fail", "keyword.control.vow"],
  ["exists", "entity.name.function.call.vow"],
  ["||", "keyword.operator.logical.vow"],
  ["var", "storage.type.let.vow"],
  ["0b1010", "constant.numeric.binary.vow"],
  ["sql", "entity.name.function.tagged.vow"],
  ["and x", "string.quoted.tagged.vow"],
  ["try", "keyword.control.vow"],
  ["catch", "keyword.control.vow"],
  ["?fallback", "keyword.other.hole.vow"],
  ["resume", "invalid.illegal.reserved.vow"],
  ["Percent", "entity.name.type.vow"],
  ["e", "variable.parameter.row.vow", 1],
  ["handler", "keyword.other.effect.vow"],
  ["Counter", "support.type.effect.vow"],
  ["spawn", "entity.name.function.call.vow"],
];
let bad = 0;
for (const [text, scope, nth] of expect) {
  const got = scopeOf(text, nth);
  if (got !== scope) {
    bad++;
    console.error(`✗ ${text}: expected ${scope}, got ${got}`);
  }
}
console.log(bad ? `${bad} grammar checks failed` : "grammar ok");
process.exit(bad ? 1 : 0);
