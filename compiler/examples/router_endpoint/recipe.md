# Test a router endpoint directly

Call the router as a function to test dispatch, request validation and response handling.

`Request.json` validates a refined name and caps the body at 256 bytes. The handler
maps `BadRequest` to a generic 400 response. `Router.handle` dispatches by method
and path, producing 404 or 405 when appropriate. There is no listener, socket,
`Serve` handler or permission to configure. Route patterns are `std/urlpattern`
patterns: `:name` matches one segment, and `/docs/**` matches any number of them,
which the handler reads as `Request.param(req, "0")`.

The tests call the actual router, not just the endpoint function. They assert the
successful response body, rejection of malformed JSON, an invalid refinement and
an oversized body, plus missing-route and wrong-method statuses. Only the tests run,
and no server is started.
