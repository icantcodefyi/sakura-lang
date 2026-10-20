# Serve a WebSocket chat room

Upgrade a route to a WebSocket and pass each member's messages to the rest of the room.

The route is an ordinary `Serve` handler. `WebSocket.handshake(req)` checks the opening
handshake without touching the connection, so a request that is not one is answered 426 with
`sec-websocket-version: 13`. `WebSocket.accept(req, options, member)` then upgrades it and runs
`member` with the `Socket`: the same `send`, `receive` and `close` a client has, closed with
1001 when `member` returns, fails or is cancelled. It returns the 101, and the server discards
what the handler returns for a connection it upgraded. When no upgrade happened it fails
`SocketError(Accept(reason))`: a browser page from an origin other than `chat.example.com` is
answered 403, and anything else, such as the `SAKURA_MAX_SOCKETS` limit, 503 with
`retry-after`. `origins` starts empty, which refuses every browser, since a browser sends the
victim's cookies with the upgrade; a client that sends no `origin` is admitted, and
authentication stays the handler's.

A `Socket` cannot be kept in handler state (`V6003`), so the room is a topic:
`WebSocket.subscribe(sock, "room")`, and `WebSocket.publish("room", message, Some(sock))`
writes to every other member without waiting, passing over one that is not reading. The runtime
has no idle timer and sends no pings: `timeout` around `receive` closes a member that has been
silent for five minutes. `max_message` caps each message at 64 KiB in both directions.

The manifest grants the listener's address; `accept` opens nothing, so it needs no entry of
its own. The tests run `Serve.scripted` around `WebSocket.scripted`, which plays the member, and
print `Get /chat -> 101 ` for the upgrade. No test opens a socket.
