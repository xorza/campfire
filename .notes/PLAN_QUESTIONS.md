# Plan questions

## Jets, End of the slice

**The question.** The item asks an agent to screenshot a jet on its runway, in a pass and parked again before the player's check. The console sends no orders, and a bot neither trains a jet nor flies a sortie (J5 leaves the jets out of a bot's army), so in a skirmish of bots no jet ever takes off, and the agent has nothing to screenshot. The Zero Hour check's jet play (J6) holds the sortie in the sim: the Raptor trains, takes off, strikes GLA's command center, goes home, lands and reloads, its ticks pinned.

**Options.**

- **The player checks alone (recommended).** You play a Tournament Desert skirmish as America, train a Raptor at an airfield and send it at a target; the agent's screenshots are dropped from this slice's end. Cost: no agent sees a fault of the drawing before you do.
- **A console order.** A new step of the Console's design gives `sim_` a request that orders a unit, as a player's input would, and the agent then flies the sortie and screenshots it. Cost: a change to the console's protocol and the MCP server, outside this slice's plan.
- **Bots fly sorties.** A step lets a bot train jets and send them at its targets, so a skirmish of bots shows them. Cost: a bot's sortie policy, outside this slice's plan.

**Blocked.** The End of the slice: the player's check and the fold of `jets.md` into the system documents.
