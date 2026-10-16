# Interaction

## Mechanism

- **One mechanism for using a unit.** Doors, the bomb, loot, capture points, shrines, vehicles and garrisons are the `use` and `enter` action kinds aimed at a unit whose type has a `use` or an `enter` section; `use` is built, `enter` planned.
- **No system of its own.** `orders` fills a use slot and the casts of `abilities` run it, so a `use` action needs both.

## Uses

- **The object gives the action.** An object's type names its `use` action and the users it allows; the order `use` puts that action in the user's use slot and runs it there, so each object brings its own outcome through the whole pipeline, as Dota 2's outposts give each hero a hidden capture ability and World of Warcraft opens a lock by an opening spell.
- **Only a user type uses.** A type uses objects only with a `uses` slot kind, so a creep or a tower never does.
- **A use is filled afresh,** so its action has one rank, an instant delivery, and no cooldown, charges, toggle or passive.
- **A held use is a channel,** ended by a new order, a block of `use`, death, or damage when it says so; its ticks take the object, so a held use adds to the object's progress, which is the object's script state.

## Planned

- **Enter and exit.** A unit rides or garrisons inside a vehicle, a transport or a building, as Zero Hour's containers need.
- **Doors** change geometry, pathing and vision blockers as they open.
