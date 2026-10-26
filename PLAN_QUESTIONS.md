# Plan questions

## Effects, End of the slice

- **Item.** A player sees Tournament Desert's shots, hits, trails and deaths in the client as the game draws them; then the slice's decisions fold into `02-engine-core.md` and `09-zero-hour.md`, and `effects.md` goes.
- **Why it waits.** The slice ends when the user sees it in the client (AGENTS.md, Workflow). No match test plays the effects' triggers, so the client is their first test.
- **Options.**
  - Play a skirmish on Tournament Desert (`cargo drive run client`), and confirm a tank's shot at its barrel, a missile's trail, a hit's burst and a building's death. Then the fold of the documents goes next.
  - Report what looks wrong. It goes to the issue log or to a new step.
- **Recommendation.** Play and confirm. The Zero Hour check finds 555 systems, 318 lists, 47 fires, 22 projectile ends, 3 trails and 596 deaths named.
- **Blocked.** The fold of `effects.md` into the system documents.
