/** Source shown in the hero animation; line 12 is the one the checker rejects. */
export const HERO_CODE = `type Email = String where is_email(it)
type SignupError = EmailTaken | Banned

fn signup(email: Email, pw: Password)
  -> User ! {Db, Clock, Random, Fail<SignupError>}
  ensures result.email == email
{
  if Db.exists(users, email) { fail EmailTaken }
  let id = Uuid.new()
  let user = User { id, email, at: Clock.now() }
  Db.insert(users, user)
  Http.post(crm_hook, user)
  user
}`;
