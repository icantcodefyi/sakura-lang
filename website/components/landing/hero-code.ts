/** Source shown in the hero animation; line 12 is the one the checker rejects. */
export const HERO_CODE = `error EmailTaken(email: Email)
error Banned
type SignupError = EmailTaken | Banned

fn signup(email: Email, pw: Password)
  -> User ! {Clock, Db, Random, Fail<SignupError>}
  ensures result.email == email
{
  if Db.exists(users, email) { fail EmailTaken { email } }
  let id = Random.uuid()
  let user = User { id, email, joined: Clock.now() }
  Db.insert(users, user)
  let _ = Http.post_json(crm_hook, user)
  user
}`;
