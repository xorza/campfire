# Plan questions

## Roads, End of the slice

- **Item.** A player sees Tournament Desert's roads in the client as the game draws them; then the slice's decisions fold into `09-zero-hour.md` and `02-engine-core.md`, and `roads.md` goes.
- **Why it waits.** The slice ends when the user sees it in the client (AGENTS.md, Workflow). D5 lays each road on the ground's triangles, and D6 draws the 3 segments of no type on `usa02` as `TwoLane`. Neither change is seen yet.
- **Options.**
  - Look at Tournament Desert in the client (`cargo drive run client`), at the report's junction near game point (1,490, 1,348) and the central plaza, and confirm. Then the fold of the documents goes next.
  - Report what looks wrong. It goes to the issue log or to a new step.
- **Recommendation.** Look and confirm. The Zero Hour check finds no decal vertex off the ground by more than 3.4 · 10⁻⁵.
- **Blocked.** The fold of `roads.md` into the system documents. The Effects batches do not read the roads' code, so they go on.

## Effects, End of the slice

- **Item.** A player sees Tournament Desert's shots, hits, trails and deaths in the client as the game draws them; then the slice's decisions fold into `02-engine-core.md` and `09-zero-hour.md`, and `effects.md` goes.
- **Why it waits.** The slice ends when the user sees it in the client (AGENTS.md, Workflow). No match test plays the effects' triggers, so the client is their first test.
- **Options.**
  - Play a skirmish on Tournament Desert (`cargo drive run client`), and confirm a tank's shot at its barrel, a missile's trail, a hit's burst and a building's death. Then the fold of the documents goes next.
  - Report what looks wrong. It goes to the issue log or to a new step.
- **Recommendation.** Play and confirm. The Zero Hour check finds 555 systems, 318 lists, 47 fires, 22 projectile ends, 3 trails and 596 deaths named.
- **Blocked.** The fold of `effects.md` into the system documents.
