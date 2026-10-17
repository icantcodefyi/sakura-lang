// Smoke check for the Sakura Lang grammar against docs/design.md §2–§3: key tokens must get the expected scopes.
import { createHighlighter } from "shiki";
import { sakuraGrammar } from "../lib/sakura/grammar";
import { sakuraDark, sakuraLight } from "../lib/sakura/themes";

const code = `/// Doc comment
import std/test.{Test}
pub opaque type Email = Email(String)
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
}
effect Task { fn spawn<T, e>(s: Scope, body: fn() -> T ! e) -> Fiber<T> ! e }
@total
fn len(xs: List<Int>) -> Int { List.len(xs) }`;

const hl = await createHighlighter({ langs: [sakuraGrammar], themes: [sakuraDark, sakuraLight] });
const tokens = hl
  .codeToTokens(code, {
    lang: sakuraGrammar.name as "text",
    theme: "sakura-dark",
    includeExplanation: true,
  })
  .tokens.flat()
  .flatMap((t) => t.explanation ?? []);
const scopeOf = (text: string, nth = 0) =>
  tokens.filter((t) => t.content.trim() === text)[nth]?.scopes.at(-1)?.scopeName;

const expect: [string, string, number?][] = [
  ["/// Doc comment", "comment.line.documentation.sakura"],
  ["still comment", "comment.block.sakura"],
  ["where", "keyword.other.contract.sakura"],
  ["it", "variable.language.sakura"],
  ["&&", "keyword.operator.logical.sakura"],
  ["error", "storage.type.sakura"],
  ["EmailTaken", "entity.name.type.sakura"],
  ["signup", "entity.name.function.sakura"],
  ["Clock", "support.type.effect.sakura"],
  ["Password", "entity.name.type.sakura"],
  ["Fail", "support.type.effect.fail.sakura"],
  ["Banned", "entity.name.type.sakura"],
  ["ensures", "keyword.other.contract.sakura"],
  ["result", "variable.language.sakura"],
  ["fail", "keyword.control.sakura"],
  ["exists", "entity.name.function.call.sakura"],
  ["||", "keyword.operator.logical.sakura"],
  ["var", "storage.type.let.sakura"],
  ["0b1010", "constant.numeric.binary.sakura"],
  ["sql", "entity.name.function.tagged.sakura"],
  ["and x", "string.quoted.tagged.sakura"],
  ["try", "keyword.control.sakura"],
  ["catch", "keyword.control.sakura"],
  ["?fallback", "keyword.other.hole.sakura"],
  ["resume", "invalid.illegal.reserved.sakura"],
  ["Percent", "entity.name.type.sakura"],
  ["e", "variable.parameter.row.sakura", 1],
  ["handler", "keyword.other.effect.sakura"],
  ["Counter", "support.type.effect.sakura"],
  ["spawn", "entity.name.function.call.sakura"],
  ["spawn", "entity.name.function.sakura", 1],
  ["opaque", "keyword.other.sakura"],
  ["Email", "entity.name.type.sakura"],
  ["test", "source.sakura"], // a module segment, not the keyword,
  ["total", "entity.other.attribute-name.sakura"],
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
