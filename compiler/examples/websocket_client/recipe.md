# Subscribe to a WebSocket feed

Open a WebSocket, subscribe to a topic and collect its updates until the server closes.

`WebSocket.connect` opens the connection and runs the callback with the `Socket`; when the
callback returns, fails or is cancelled the connection is closed (1001 if it is still open), and
returning the socket from the callback is `V6003`. There are no event listeners: a message or
the close is a value `WebSocket.receive` returns, and `WebSocket.messages(sock)` is the same as a
source that ends at the close, so `filter`, `map` and `to_list(max)` read it with a stated
bound. `max_message` caps each message at 64 KiB in both directions; a larger one from the
server ends the connection with 1009.

The manifest grants `feed.example.com:443`, the port a `wss` URL uses by default; a `main` that
declares `WebSocket` with an empty `net` list exits 75 at startup. The tests run against
`WebSocket.scripted`: `Reply` answers the subscription, `Push` sends updates, `Close` ends the
feed, and a send the script does not expect fails with `SocketError(Failed(...))` instead of
waiting. No test opens a socket.
