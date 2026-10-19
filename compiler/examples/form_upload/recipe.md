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

The tests send the upload through `Http.scripted`, route an upload request through the
receiving endpoint with `Router.handle`, and check the limit and content-type refusals.
Request bodies are text in this version, so the file here is CSV; binary file bytes need the
byte bodies planned for HTTP requests. No permissions are needed.
