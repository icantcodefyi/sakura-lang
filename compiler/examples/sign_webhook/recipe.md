# Verify a signed webhook with HMAC

Check that a webhook delivery carries a valid HMAC-SHA-256 signature before acting on it.

A webhook provider signs each body with a secret it shares with you and sends the tag in a
header, here `X-Signature` as Base64 (Shopify's `X-Shopify-Hmac-Sha256` has this shape).
`webhook_key` imports the secret as a `CryptoKey` that may only `Verify`: the material stays
a `Secret`, so the key is never shown, logged or serialized, and `export_key` refuses it.
`key.verify(tag, body)` recomputes the HMAC and compares it in constant time, so the time a
check takes does not reveal how many leading bytes of a forged tag were right. A missing
header or one that is not Base64 is not authentic, and the endpoint answers 401 without
saying why.

The tests sign deliveries the way the provider does and check that the right secret is
accepted, that a changed body, another secret, a missing header and a malformed one are
rejected, and that the receiving key cannot sign. They run the router as a function, with no
server, and need no effects or permissions: `std/crypto` is pure. In an application the
secret comes from `Env.secret`, which needs an `env` entry in `[permissions]`.
