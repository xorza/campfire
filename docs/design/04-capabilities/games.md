# Games

- **A game is a package.** The capabilities it declares, its data and its mode scripts; nothing ties a capability to a genre. The engine grows for the games it imports, each through a module of the `import` app and a rules package ([Zero Hour](../12-zero-hour.md)).

## Imported games

| Game | Capabilities | Rules package | Tick rate |
| --- | --- | --- | --- |
| Command & Conquer: Generals – Zero Hour | `combat`, `stats`, `abilities`, `projectiles`, `areas`, `orders`, `navigation`, `vision`, `production`, `interaction`, `progression` | General's powers and sciences, money over time, superweapons, the skirmish AI | 30 Hz |
| Worms 4: Mayhem | `combat`, `stats`, `projectiles`, `areas`, `character`, `physics`, destructible terrain, the `spatial` metric | Turns, weapons, missions | Not yet known |

- **Phases, win conditions and economy are mode scripts** in every game.

## The genres in the model

- **Every genre in the same terms.** A MOBA, a shooter, an RTS, an MMO and a battle royale differ only in which control, actions, pools, tags, relations, space and vision they declare; a battle royale's zone is an area unit, an MMO's quest giver a unit with a `use`, and a first-person commander one package of `character`, `hitboxes`, `orders` and `navigation` at one tick rate.

## Main risks

- **Unit counts.** A large Zero Hour battle, eight armies at 30 Hz, must fit the tick budget and the bandwidth; the benches measure 1,000 units, and the server sends only what each group sees.
- **Rhai's speed.** The rules package's cost at a battle's scale is measured before it grows.
- **Parity.** A difference a player notices shows only where a player meets it, as no test plays the original ([Zero Hour](../12-zero-hour.md)).
- **No copy in CI.** An imported package exists only on a machine that holds the game.
