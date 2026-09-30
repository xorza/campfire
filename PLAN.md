# Plan — next steps

Open items only, in order. Remove an item when it is done. Each step ends with the check chain passing for the crates it touches, and a stop for review. Stage context: [ROADMAP.md](ROADMAP.md); design: [design/](design/).

1. **Structures clear of paths**: with collision and no steering, a unit that does not walk blocks every walker on a path through it for good; both maps had their towers, and the 3v3 its inhibitors, on or next to their lanes, which moved beside them. The mode's load refuses a map whose structure's body comes closer to a path than the widest body of a unit that walks, among the mode's unit types and avatars, exactly, in fixed point. Test: a map with a tower on its lane fails its load, and one 0.1 m clear of the widest walker loads.
2. **Vertical slice playtest, third round**: the same 1v1 on LAN, with collision. Units that block each other and body block are new to the feel; write down whether the match reads and plays well. This closes stage 3, or names what is missing.
