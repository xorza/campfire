# Plan questions

The items of [the plan](PLAN.md) that wait for a decision of the user.

## Veterancy and generals: end of the slice

- **Item.** A player of Tournament Desert sees a tank turn veteran after its kills, reaches rank 3, buys the Paladin and sees its button come on, buys Emergency Repair and fires it onto damaged vehicles from the command center, in the client; then the slice's decisions fold into the system documents and `generals.md` goes.
- **Why it waits.** The slice ends when the user plays it in the client ([AGENTS.md](../AGENTS.md#workflow)). The play is the user's, and the fold of its decisions follows the play.
- **Options.** 1. The user plays the slice with `cargo drive run client`, then the decisions fold into the system documents. 2. The decisions fold now, and the user plays the slice later.
- **Recommendation.** Option 1, as the play can change a decision before it folds.
- **Blocked.** Nothing: the later slices need the slice's code, not its play.

## Combat response: end of the slice

- **Item.** A player of Tournament Desert sees their idle tanks go after a rocket buggy that hits one of them and come back, a driving tank fire on the move, and a Comanche switch weapons by its target, in the client; then the slice's decisions fold into the system documents and `combat-response.md` goes.
- **Why it waits.** The slice ends when the user plays it in the client ([AGENTS.md](../AGENTS.md#workflow)). The Zero Hour check's plays of a turret, a retaliation and a Comanche are CR6's.
- **Options.** 1. The user plays the slice with `cargo drive run client`, then the decisions fold. 2. The decisions fold now, and the user plays the slice later.
- **Recommendation.** Option 1, as the play can change a decision before it folds.
- **Blocked.** Nothing: Transports and garrisons needs the slice's code, not its play. The plays of CR6, which the user chose on 28 October 2026, come first, so the client play follows the check's.
