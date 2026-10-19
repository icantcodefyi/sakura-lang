# Build a URL with Url.parse and SearchParams

Parse a base URL, set its path and query parameters, and resolve links against it.

`Url.parse` is the WHATWG URL parser behind the web's `new URL(s)`: it lowercases the
scheme and host, drops a default port, resolves `.` and `..` segments and percent-encodes
what each component cannot hold, returning `UrlParts` or a `UrlError` with its reason.
Each `with_*` function returns a new value, so the base is never changed. `Url.search`
reads the query as `SearchParams`, an ordered multimap whose `set` replaces the first pair
of a name and keeps the rest of the query; `with_search` writes it back, encoding a space as
`+` and `&`, `=` or `#` in a value as `%26`, `%3D` and `%23`. `to_url` gives the `Url` that
`Request.of` takes. `Url.resolve` is `new URL(link, base)`, the step that turns a redirect's
`Location` header or a relative `next` link into an absolute URL.

The tests check that other parameters survive, that query text cannot add a parameter, that
relative, scheme-relative and absolute links resolve, and that a path-only `Url` (valid for
`Request.url`, but not absolute) and a non-ASCII host fail with `Relative` and `Idna`. No
effects or permissions are needed: a URL value grants nothing; `net` is checked when a
request is sent.
