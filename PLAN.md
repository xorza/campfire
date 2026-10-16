# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## The Zero Hour fingerprint on macOS

`zero-hour-check`: `pins.rs`. It needs `GENERALSZH_DATA` and a Mac.

- [ ] checks/zero-hour-check/src/pins.rs:17-19 — `FINGERPRINTS.game` holds `9a322cdb435b7860b4a89d526296087da2b3ea553ce23edb038831b43c55c90f`, which a run on Linux gave after 61c471a8 made `SinCos::of_degrees` round exactly and moved the `max` of four maps' bounds in their last bits; no run on macOS gave it yet, as design 12, D4 requires of every pin. → `./drive.py zero-hour` on macOS gives the same value, or the difference goes to the issue log under **Import**.
