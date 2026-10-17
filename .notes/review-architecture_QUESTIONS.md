# Questions of the architecture plan

Each question waits for the user's decision. The plan's items point here.

## Q10. Batch 5.3, one action pipeline: review of its proposal

- **Item:** 5.3, K-17, C-3 and E-5. The proposal is [action-pipeline.md](../docs/design/action-pipeline.md), written as Q7 decided.
- **Why it waits:** the batch flips `stats` above `actions` and moves the stat effects and the modifier ways into `stats`. AGENTS.md asks a change of this size for a design the user reviews before it is built.
- **Options:**
  1. **Accept the proposal (recommended).** One start and one commit in `actions`, each kind commits at its moment of today, four contracts in `units`, and no golden moves.
  2. **Accept it with one commit moment for every kind.** Trains and builds then pay at their end, as casts do. Cost: the goldens of every match with a train or a build move.
  3. **Change the proposal.** The user names the decisions to change.
- **Blocked:** 5.3, and 5.1, which needs 5.3's callbacks.
