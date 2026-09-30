# Plan — next steps

Open items only, in order. Remove an item when it is done. Each step ends with the check chain passing for the crates it touches, and a stop for review. Stage context: [ROADMAP.md](ROADMAP.md); design: [design/](design/).

1. **LAN check on request**: `campfire-client --bot <orders file>` plays the same scripts without a window or rendering. A command outside the normal test suite starts the real `campfire-server` and two bots as processes over WebTransport on `127.0.0.1` for a match of about 3 s, reads their JSON logs with `serde_json`, and runs the verifier: every order was logged and took effect in its stamp tick, no join or input was refused, and the verifier gives the server's final hash. Test: the check fails when the raw client has no local address, and passes without that flaw.
