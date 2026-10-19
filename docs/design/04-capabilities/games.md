# Games

- **A game is a package.** The capabilities it declares, its data and its mode scripts; nothing ties a capability to a genre. The engine grows for the games it imports, each through a module of the `import` app and a rules package ([Zero Hour](../12-zero-hour.md)).

## Imported games

| Game | Capabilities | Rules package | Tick rate |
| --- | --- | --- | --- |
| Command & Conquer: Generals – Zero Hour | `combat`, `stats`, `abilities`, `projectiles`, `areas`, `orders`, `navigation`, `vision`, `production`, `interaction`, `progression` | General's powers and sciences, money over time, superweapons, the skirmish AI | 30 Hz |
| Worms 4: Mayhem | `combat`, `stats`, `projectiles`, `areas`, `character`, `physics`, destructible terrain, the `spatial` metric | Turns, weapons, missions | Not yet known |

- **Common rules are data.** A game's phases and its special rules are its mode scripts, and a rule many games share is a data section whose pass visits what changed, as the tick's deaths.
- **Victory by the last unit of a tag.** The mode's `[victory] counts = "<tag>"`: a player whose last living unit of the tag dies is out of play, and the last team whose players hold such units wins, none a draw; `defeat = "kill_all"` kills the player's other units, as Zero Hour's `Player::killPlayer` does, and `"keep"`, the default, leaves them. Zero Hour's skirmish counts `MP_COUNT_FOR_VICTORY` buildings under `VICTORY_NOBUILDINGS`, as OpenRA marks `MustBeDestroyed`. Refused: a filter of the counted units, which no source game needs, as each marks a kind of its own.
- **A kill's bounty.** A unit type's `[bounty] resource` with an `amount` pays that amount to the owner of its killer; with a `cost`, it pays the killer's player's share of the cost, rounded up, a share that starts at the mode's `[bounty] share` and that a script raises with `ctx.set_bounty_share`, as a power does. A death pays only when its killer passes the mode's `[bounty] killer`, a filter from the victim's side. Warcraft III and the 3v3 pay a type's fixed amount, and Zero Hour's `Player::doBountyForKill` pays the player's `m_cashBountyPercent` of the victim's build cost, rounded up, as OpenRA's `GivesBounty` pays its `Percentage` of the sell value. Refused: an amount alone, which leaves Zero Hour's Cash Bounty a script; and a share alone, which gives the 3v3's creeps a cost they do not have. A hero kill's first blood and assist gold, the 3v3's own, stay its script.
- **A death's experience.** Progression's mode `[experience]` names the track, and `to = "killer"` gives a death's experience to its killer, none to an ally's, as Zero Hour and OpenRA's `GivesExperience` give it, or `to = { radius, filter }` splits it evenly, rounded to the nearest, among the living units of the track the filter selects for the victim whose bodies come within the radius of its point, as Dota 2 and the 3v3 do. A unit type's `[experience] gives` is a number, a list by the victim's level on the track, as Zero Hour's `ExperienceValue`, or `{ base, per_level }`, `base` and `per_level` for each level, as the 3v3's heroes. Refused: the split alone, which leaves Zero Hour's experience a script; and the values as unit params, which no reference lists.

## The genres in the model

- **Every genre in the same terms.** A MOBA, a shooter, an RTS, an MMO and a battle royale differ only in which control, actions, pools, tags, relations, space and vision they declare; a battle royale's zone is an area unit, an MMO's quest giver a unit with a `use`, and a first-person commander one package of `character`, `hitboxes`, `orders` and `navigation` at one tick rate.
- **The mode's shape from data, planned.** With the slice that needs each: teams from the session's terms in place of the manifest; a result of winners and placements in place of one team or a draw; faction offers; options that data reads as well as scripts; `team.enemies` in place of a single enemy team; one `Loadout` shape.

## Main risks

- **Unit counts.** A large Zero Hour battle, eight armies at 30 Hz, must fit the tick budget and the bandwidth; the benches measure 1,000 units, and the server sends only what each group sees.
- **Rhai's speed.** The rules package's cost at a battle's scale is measured before it grows.
- **Parity.** A difference a player notices shows only where a player meets it, as no test plays the original ([Zero Hour](../12-zero-hour.md)).
- **No copy in CI.** An imported package exists only on a machine that holds the game.
