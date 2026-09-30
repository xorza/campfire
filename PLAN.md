# Plan — next steps

Open items only, in order. Remove an item when it is done. Each step ends with the check chain passing for the crates it touches, and a stop for review. Stage context: [ROADMAP.md](ROADMAP.md); design: [design/](design/).

1. **Combat core** in `kit-moba`: teams, health, an attack order with range and windup, damage, death. A tower attacks the nearest enemy in range; creeps spawn in waves and walk the lane waypoints. Test: hand-computed hits, deaths and tower targets, and a log with creeps and a tower replays to the same hashes.
2. **Rhai host** in `script`: the fixed hashing seed, operation limits, no floats, `Num` in scripts, and one hero ability from `packages/moba` run through the kit's cast checks, with all-or-nothing effects. Test: the ability's effect is exact; a call over its limit fails the same way in every build and changes nothing.
3. **Fog of war**: grid visibility per team, and each client receives only the units its team sees, through Lightyear's replication visibility. Test: an enemy outside sight is not in the client's `World`, and appears on the tick it comes into sight.
4. **LAN play**: `server` and `client` binaries over WebTransport, with the certificate hash pinned and the connect challenge of design 05, which checks the delegation's expiry against the server clock; the client signs with auxiliary randomness from the OS, which `net` lacks today. The client draws capsules, predicts its own hero and interpolates the other units. Exit: two machines play a 1v1 lane, and the log verifies on another OS.
