# Test a router endpoint directly

Call the router as a function to test dispatch, request validation and response handling.

`Request.json` validates a refined name and caps the body at 256 bytes. The handler
maps `BadRequest` to a generic 400 response. `Router.handle` dispatches by method
and path, producing 404 or 405 when appropriate. There is no listener, socket,
`Serve` handler or permission to configure.

The tests call the actual router, not just the endpoint function. They assert the
successful response body, rejection of malformed JSON, an invalid refinement and
an oversized body, plus missing-route and wrong-method statuses. The harness uses
test mode and never starts a server.
