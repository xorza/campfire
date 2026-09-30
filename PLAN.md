# Plan — next steps

Open items only, in order. Remove an item when it is done. Each step ends with the check chain passing for the crates it touches, and a stop for review. Stage context: [ROADMAP.md](ROADMAP.md); design: [design/](design/).

1. **LAN play**: `server` and `client` binaries over WebTransport, with the certificate hash pinned and the connect challenge of design 05, which checks the delegation's expiry against the server clock; the client signs with auxiliary randomness from the OS, which `net` lacks today. The client draws capsules, predicts its own hero and interpolates the other units. Exit: two machines play a 1v1 lane, and the log verifies on another OS.
