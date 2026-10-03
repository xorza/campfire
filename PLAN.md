# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Stage 5: MOBA mechanics

Progression first: points and the `learn` order ([Progression](docs/design/04-capabilities/progression.md#rules)). Perks wait for stage 9. After it, the verifier on a 3v3 log.

5. **P5. The scripted 3v3 learns** (runner): the 3v3's scripted players spend their points by the learn order, as the rank levels allow, and the match test checks ranks and points at chosen ticks, among them a learn refused for its level. The hero casts stay for the roadmap's 3v3 abilities item. The 3v3 golden is blessed.
6. **P6. Learning in the client** (client, test lane mode): Ctrl and a cast key sends the learn order for that key's slot, and the HUD shows the unspent points and each slot's rank. The lane mode gets a `level` track and experience for kills, and its heroes learn by points in place of its `ctx.learn`, so a LAN playtest can learn. Tests: the key to the order, and the lane mode's scripted match with a learn; its goldens are blessed.
7. **P7. The verifier on a 3v3 log** (verifier): a test verifies the session log of the scripted 3v3 match, learn orders included, with the package store and the seed checks the lane mode's log has, so the new order replays to the same hash.
