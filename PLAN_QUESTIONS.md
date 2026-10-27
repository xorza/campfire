# Plan questions

## Effects, End of the slice

- **Item.** A player sees Tournament Desert's shots, hits, trails and deaths in the client as the game draws them; then the slice's decisions fold into `02-engine-core.md` and `09-zero-hour.md`, and `effects.md` goes.
- **Why it waits.** The slice ends when the user sees it in the client (AGENTS.md, Workflow); no test sees the client's frames.
- **Options.**
  - Play a skirmish on Tournament Desert (`cargo drive run client`), and confirm a shot at its barrel, a missile's trail, a hit's burst, a building's explosion, and a tank's death in its phases. Then the fold of the documents goes next.
  - Report what looks wrong. It goes to the issue log or to a new step.
- **Recommendation.** Play and confirm.
- **Blocked.** The fold of `effects.md` into the system documents.

## Console, End of the slice

- **Item.** Claude Code connects to the project's `.mcp.json` and lists the tools; the user watches the agent start a local match of the MOBA test mode, run it to a tick, table its units and show a screenshot of one; in a local Zero Hour skirmish of bots alone, the agent finds the walkers stuck at a tick with the tools alone; then the slice's decisions fold into the system documents, and `console.md` goes.
- **Why it waits.** Claude Code's own connection, its approval of `.mcp.json` and the user's watch are the slice's end (AGENTS.md, Workflow). A session of this plan cannot restart its own MCP servers.
- **What stands.** `cargo drive run mcp` builds `source/target/release-dev/campfire-mcp`; a script drove the built server over its stdio through `game_start` of a local lane match, `sim_status`, `sim_run` to tick 60, `sim_units`, three `view_screenshot`s (the window, the box round two heroes, and a rectangle of the last image) and `game_stop`, each as its test expects.
- **Options.**
  - Restart Claude Code in the repository, approve `.mcp.json`, and ask an agent for the slice's loop on the MOBA test mode, then on a Zero Hour skirmish of bots. Then the fold of the documents goes next.
  - Report what fails. It goes to the issue log or to a new step.
- **Recommendation.** Restart, approve and watch the loop.
- **Blocked.** The fold of `console.md` into the system documents.

## Wrecks, End of the slice

- **Item.** An agent screenshots a destroyed Crusader, a destroyed Comanche and a driving tank through the console; then a player sees the hulks, debris, dust, track marks and an aircraft's fall in the client; then the slice's decisions fold into `02-engine-core.md` and `09-zero-hour.md`, and `wrecks.md` goes.
- **Why it waits.** The slice ends when the user sees it in the client (AGENTS.md, Workflow), and the agent's part found no fight to screenshot.
- **What stands.** A Tournament Desert skirmish of two builder bots from prebuilt bases, run through the client's console to tick 9,120, held no fight: each army stood idle at its base, so no Crusader or Comanche died and no tank drove. A Chinese supply truck drove, and its screenshot shows its tire track behind it on the drawn ground. The tests hold the falls, the dust's scales and the track's edges at hand-computed values.
- **Options.**
  - Play a skirmish on Tournament Desert (`cargo drive run client`), drive a Crusader, and destroy a Crusader and a Comanche; confirm the hulk, the thrown turret and debris, the dust, the track marks and the Comanche's spiral and blow-up. Then the fold of the documents goes next.
  - Report what looks wrong. It goes to the issue log or to a new step.
- **Recommendation.** Play and confirm.
- **Blocked.** The fold of `wrecks.md` into the system documents.
