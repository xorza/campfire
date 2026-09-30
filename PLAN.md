# Plan — next steps

Open items only, in order. Remove an item when it is done. Each step ends with the check chain passing for the crates it touches, and a stop for review. Stage context: [ROADMAP.md](ROADMAP.md); design: [design/](design/).

1. **Every death in the server's log**: the server logs a death from `Added<Dead>` after the tick, and a unit that despawns when it dies, such as a creep, is gone by then, so the first playtest's server log named no creep's death. Combat's `Deaths` already records each death of the tick, with its killer and assisters, and keeps it until the next tick's damage. The server logs the tick's deaths from `Deaths`, with each killer; `Deaths` records each dead unit's team and whether a player owned it, which the log names and a despawned unit no longer holds. Test: a creep that dies and despawns, and an avatar that dies and stays, each give one death line with its killer.
2. **Vertical slice playtest, second round**: the same 1v1 on LAN, Linux against macOS, with the server on a third machine. Write down what makes the match fun or not. This closes stage 3, or names what is missing.
