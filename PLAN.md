# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

**Slice 5, units that drive.** The user starts Tournament Desert in the client, finds America's command center and dozer at their start, selects the dozer, and drives it round the cliffs to the other start, at its speed and turn rate, on the drawn ground. The design is [Units that drive](docs/design/17-units-that-drive.md); the facts are [Zero Hour research](docs/research/zero-hour.md#movement). The Zero Hour check plays the drive, and loads every imported map.

**Optimization, in a session the user starts.** The change-driven passes wait in [optimization](.notes/optimization.md), with what the first session measured.

## 1. Places are vectors

`sim`: `position`; `geometry`; `script`, `units`, `mode` and every capability that reads a place; `client-files`; the client; design 09 D7 and design 08. It stands before U8, whose mode script moves a marker's point by an offset, which `Pos` cannot.

### P1. A place is a `Vec3`

(Determinism core D7; the [Direction](AGENTS.md#direction)'s one word, one meaning.) The checked type `Position` goes. A place is a `Vec3` in every API, geometry and script. The sim's part that holds a unit's place is `Place(pub Vec3)`, a component of no rule, of the same state name `sim.position` and the same encoding, so no state hash moves. The world's bound, 2²⁰ m, stays as the constant `Place::BOUND`, and the map's `Bounds` holds it, not the type:

- `Bounds` holds the height too: `contains` also checks `|y| ≤ Place::BOUND`, so the restore's `within_bounds`, which a decode of untrusted bytes runs, refuses a place past the world's bound on every axis.
- A place enters the sim only through a check of `Bounds`: a map's load, a script call (`spawn_unit`, `order_move`, `teleport` and their kin, which now also take a script's vector of any value and raise an error past the bounds), a client's order and a restore. Each geometry function that took two `Position`s states that its places lie within the bounds, and keeps its `expect`s of no overflow on that.
- `Metric::step_toward`, `Bounds::clamp`, `ground_point` and the rest give a `Vec3`, with no `Option` to unwrap.
- The script API drops `Pos`. `unit.pos`, `marker.pos` and every other place give a `Vector`; `distance_to`, `within` and `direction_to` move to `Vector`; the call `vector(x, y, z)` makes one; the script state's type `pos` becomes `vector`, which no package uses.
- Design 09 D7 drops `Position`'s conversions: a triple converts to a `Vec3` as before, and no type checks the bound on conversion.

Check: no golden moves; a restore of a unit at a height past the bound is refused; a script's `spawn_unit` at a vector past the bounds raises its error; `marker.pos + vector(0, 0, 5)` spawns 5 m along `+z`; the reference is generated again.

## 2. Zero Hour's units drive

`import`: `zero_hour`; `source/packages/zero-hour`; `checks/zero-hour-check`. It needs the turn rate, the load's rules and the selection to play.

### U8. The skirmish's start

(`import`, the rules package; Units that drive D6.) The import reads `PlayerTemplate.ini`, tags each `Player_<n>_Start` and `Player_<n>_Rally` waypoint `start`, and gives the mode `[navigation] spawn_tags = ["start"]` and the `[params]` of America's start; the rules package's mode script spawns, for the player at start position `n`, the building at `Player_<n>_Start` and each starting unit at `Player_<n>_Rally`, or at the building's point moved `start_offset` along `+z`, as `marker.pos + vector(0, 0, ctx.p.start_offset)` of P1, owned by the player. Check: the import's fixture install's params and tags; the Zero Hour check finds the command center at `Player_1_Start` and the dozer clear of its box, owned by the player; the dozer, ordered to `Player_2_Start`, arrives within a bound of ticks the check pins and logs, and never stands in a wall's cell; the user drives it in the client.

### U9. The Zero Hour check compares fingerprints and plays the parity suite

(`checks/zero-hour-check`, the parity oracle; Zero Hour D5, D7, D8.) The Zero Hour check compares the fingerprint of its import with the one the other OS gave, and plays the parity suite against the oracle's frames, as design 12 D7 says. It waits for the oracle, the fork of GeneralsX that writes each logic frame. Check: a run that names no install fails; a run on Linux and one on macOS give one fingerprint; the suite's replays meet the measure.
