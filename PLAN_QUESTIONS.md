# Plan questions

## Effects, End of the slice

- **Item.** A player sees Tournament Desert's shots, hits, trails and deaths in the client as the game draws them; then the slice's decisions fold into `02-engine-core.md` and `09-zero-hour.md`, and `effects.md` goes. It stands after E5.
- **Why it waits.** The slice ends when the user sees it in the client (AGENTS.md, Workflow). No match test plays the effects' triggers, so the client is their first test.
- **Options.**
  - Play a skirmish on Tournament Desert (`cargo drive run client`), and confirm a tank's shot at its barrel, a missile's trail, a hit's burst, a building's explosion, and a tank's death in its phases: its fire, then its explosion. Then the fold of the documents goes next.
  - Report what looks wrong. It goes to the issue log or to a new step.
- **Recommendation.** Play and confirm. The Zero Hour check finds 555 systems, 318 lists, 47 fires, 22 projectile ends, 3 trails and 806 deaths named.
- **Blocked.** The fold of `effects.md` into the system documents.

## Wrecks, End of the slice

- **Item.** A player sees Tournament Desert's hulks, debris, dust and track marks, and an aircraft's fall, in the client as the game draws them; then the slice's decisions fold into `02-engine-core.md` and `09-zero-hour.md`, and `wrecks.md` goes.
- **Why it waits.** The slice ends when the user sees it in the client (AGENTS.md, Workflow); the client's tests compute the motion, the client alone shows it.
- **Options.**
  - Play a skirmish on Tournament Desert, destroy a Crusader and a Comanche, and drive a tank: confirm the hulk and its sink, the turret and the debris, the helicopter's spiral and blow-up, the dust and the track marks. Then the fold of the documents goes next.
  - Report what looks wrong. It goes to the issue log or to a new step.
- **Recommendation.** Play and confirm.
- **Blocked.** The fold of `wrecks.md` into the system documents.

## Console, End of the slice

- **Item.** Claude Code connects to `campfire-mcp`, and the user watches the agent run a local match to a tick, table its units and show a screenshot; then the slice's decisions fold into their systems' documents, and `console.md` goes.
- **Why it waits.** It needs the user's approval of the project's `.mcp.json` in Claude Code and the user's watch of the window.
- **Options.**
  - Run `cargo drive run mcp`, approve `.mcp.json`, and ask the agent to start a match and show a unit. Then the fold of the documents goes next.
  - Report what fails. It goes to the issue log or to a new step.
- **Recommendation.** Run it and confirm.
- **Blocked.** The fold of `console.md` into the system documents.
