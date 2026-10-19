# Upload a file as multipart form data

Build a `multipart/form-data` body with `Form`, send it, and parse it back within limits.

`Form` is the web's `FormData` as a value: an ordered list of fields, each `Text` or a
`File` holding a `FileData` (the web's `File`: a `Blob` of bytes and a media type, a name and
a time). `Form.encode_multipart` writes HTML's form-submission layout, and `Form.boundary()`
draws a fresh boundary from `Random`, so tests run under `Random.seeded` and get the same
bytes every time. The request's `content-type` names the boundary. The receiving side calls
`Form.parse_multipart` with a required `FormLimits`: the part count, the whole body and each
part are bounded, and a body over a bound, or one with the wrong content type, is a typed
`FormError` the handler turns into `400`. A file's name is data, never a path that is opened.

The request body is a `Payload`, here `Bytes`, so a file may hold any bytes. The tests send
a CSV upload through `Http.scripted`, replay a binary PNG upload and its binary answer from a
cassette (`Http.recording` stores a body that is not UTF-8 as Base64), route an upload request
through the receiving endpoint with `Router.handle`, and check the limit and content-type
refusals. No permissions are needed.
