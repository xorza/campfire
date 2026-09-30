# Structural redesign: one owner for each fact, typed data at the edge, one script path

This proposal answers every item of `.notes/review.md`. It has three parts: the principles, the
target shape of each area, and the order of work. Paths are from `source/crates/`.

## Principles

1. **One owner for each fact.** A name, a kit, a lane, a rule or a type has one place. Every
   other place asks that one.
2. **Typed where it enters.** A manifest or data value with rules becomes a checked type when it
   is read (`AGENTS.md`). A later check never repeats what the type already guarantees.
3. **Capabilities own their graph.** Which capabilities exist, which need which, and how they
   install is capabilities' knowledge. The runner and the client ask for a declared set.
4. **One path for script calls.** Every hook runs through one batch in the `units` core, which
   owns the host, the budgets and the failures.
5. **Reading packages is not building a match.** The runner reads and checks packages once, and
   builds each match from references to them.

## Target shape

### 1. Script calls: one batch, a budget for each player

Now: `resolve_casts`, `think` and `Calls::batch` each take the host out of the world, copy a
budget, run calls, record failures by hand, and put the host back. Mode inputs and casts share
one `input` pool, so one player can make another's cast fail.

Target, in `capabilities/src/units/`:

```rust
/// A group of script calls in one stage: the view as the stage began, the host taken out of the
/// world for the group, and the pools the calls draw from.
pub(crate) struct ScriptBatch<'w> { world: &'w mut World, host: ScriptHost }

/// The pool a call draws from.
pub(crate) enum Pool { Player(u32), Think, Mode }

impl ScriptBatch<'_> {
    pub(crate) fn open(world: &mut World, view: &View) -> ScriptBatch<'_>;
    pub(crate) fn call(&mut self, pool: Pool, script: ScriptId, hook: Hook,
                       args: impl FuncArgs) -> Result<Dynamic, ScriptError>;
    pub(crate) fn record(&mut self, unit: Option<StableId>, hook: Hook, error: ScriptError);
    pub(crate) fn world(&mut self) -> &mut World;
}
// Drop puts the host back.
```

- `ScriptBudgets` holds a budget for each player slot, and the `think` and `mode` pools. Each
  tick every budget starts full.
- The manifest's `script_limits` becomes `{ per_call, player, think, mode }`: `player` is each
  slot's operations a tick, at least `per_call`. A player's mode inputs and casts draw from that
  player's budget, so a player's calls can spend only their own. A cast draws from the pool of
  its caster's controller.
- The three systems keep their own order and their own reaction to `TickBudget`: an AI unit and
  a timer stay due, a mode input and a cast are recorded as failed.
- Design 02 changes: "one pool per player for the calls that player causes", in place of the
  shared `input` pool.

### 2. Capabilities: a checked set that installs itself

Now: the needs graph is in `runner/src/load_check.rs`, the install order is an array in
`runner/src/mode_packages.rs`, and `net/src/sim_client/mod.rs` has its own list and fake
limits.

Target, in `capabilities/src/capability_set.rs`:

```rust
/// A mode's declared capabilities: none twice, not `mode`, and each with the ones it needs.
pub struct CapabilitySet(/* bit set over sim's Capability */);

impl CapabilitySet {
    pub fn new(declared: &[Capability]) -> Result<CapabilitySet, CapabilityError>;
    pub fn contains(self, capability: Capability) -> bool;
    /// Installs the core and every declared capability the release runs, in the graph's order.
    /// `scripts` is `None` on a client, which runs no scripts.
    pub fn install(self, world: &mut World, schedule: &mut Schedule,
                   registry: &mut StateRegistry, scripts: Option<ScriptLimits>);
}
```

- The needs graph (`Projectiles` and `Abilities` need `Combat`, `Orders` needs `Combat` and
  `Navigation`) is a `const fn needs(Capability) -> &'static [Capability]` beside it.
- `ModeManifest::capabilities` deserializes into a `CapabilitySet`, so a manifest that declares
  `mode`, repeats one or misses a need fails where it is read.
- `Units::install` takes `Option<ScriptLimits>`. With `None` it inserts no host and no budgets,
  and `Control::install` then registers no AI API. The client installs from the session's
  declared set with `None`, and `CLIENT_LIMITS` goes away. The client learns the set from the
  mode it plays; until LAN play gives it the packages, `LocalPair` passes the set it loaded.

### 3. The manifest: typed at the read

In `capabilities/src/mode/manifest.rs`:

- `max_move_speed: Speed`, a positive `Num` newtype, in meters a second. `KitRules` takes it
  directly; the `expect` in `ModePackages::install` and the `MoveSpeedCap` check go away.
- `tick_hz: TickRange`, a checked `min ≤ default ≤ max` of `NonZeroU32`, with
  `contains(NonZeroU32)`. `Session::start` asks it, and `LoadProblem::TickRange` goes away.
- `script_limits: ScriptLimits`, checked when read: each pool at least one call. Its
  `LoadProblem::PoolTooSmall` becomes a read error.
- `LoadCheck::manifest` then keeps only the engine release of every package.

### 4. Data values: `Ranked` everywhere, a param in any number field

- `Param` becomes `Ranked(Ranked<Scalar>) | Scaling(Scaling)`, and `param_value` calls
  `Ranked::at`.
- `AbilityData::cooldown_ms`, `cost`, `cast_time_ms` and `range` become `Ranked<Number>` (a
  `Range` gets the same `{ param }` form). `AbilityBook` resolves them at the cast's rank through
  the ability's params, as it resolves `ctx.p`.
- The rank rule has one home: `AbilityData::check_ranks(ranks: usize)`, which counts every
  per-rank array, params and number fields alike. The load check calls it with 5, 3 or 1.
  `AbilityBook::load` trusts it, and `AbilityError::RankCounts` goes away.

### 5. Filters: one data type, resolved once

- `FilterData` (a relation and an optional tag name) is the one data form. `FilterSyntax` becomes
  private to it and to the run-time parse of a script's literal.
- `Targeting::Unit(FilterData)`: `enemies:hero` loads. When an ability loads, its targeting
  resolves to the run-time `Filter` against `UnitTypes`, and the cast's check uses
  `Filter::selects`. The unit types load before the abilities, so every tag is known then.
- The load check tests a script literal with `FilterData::parse`, and a data filter's tag as now.

### 6. Facts with one owner

- **Unit type names** move into `UnitTypes`: `Units::load_type(world, name, &data)`.
  `ModeBook::unit_types` goes away; the mode's `ctx` looks names up through the view, which owns
  `UnitTypes`. `unit.unit_type` then has its value.
- **Kits** live only in `ModeBook::kits`. `HeroSetup` and `UnitTypeSetup` give theirs up at
  install.
- **Lanes:** every unit on a lane has `OnLane`. `LaneWalker` keeps only its direction and its
  next waypoint, and `follow_paths` reads `OnLane`. `read_extras` reads one component.
- **Fingerprint** moves into `protocol` as `Fingerprint`, and `SessionTerms::mode` and
  `dependencies` use it. `content` computes it. The four conversions go away.
- **Reach on the ground plane** becomes `Position::within_ground(other, radius)` in `sim`, and
  `ground_offset` becomes private to it. `nearest_visible` keeps its offset for the sort through a
  `Position::ground_offset` it can call.
- **Command lists:** `Command::payload(capability, bodies)` in `sim`, which `Order::payload` and
  `ModeInput::payload` call.
- **The time of a stage:** `SimTick::start()` (the Inputs stage, and the match start of tick 0)
  and `SimTick::end()` (the Mode stage) give the time a timer counts from. `Timers::due` takes
  that time, and `mode_inputs`, `run_timers` and `Mode::start` stop writing `+ 1` and `0`.

### 7. Reading packages, then building matches

In `runner/src/`:

- `ModePackages` only reads and checks. It exposes what a match needs, by reference.
- A new `MatchBuild` (in `match_build.rs`) takes `&ModePackages`, a world, a schedule, a registry
  and the player count. It installs the declared set, loads the unit types, heroes, spells and
  AI, and installs the mode. `load_hero` and `load_ability` become its methods.
- `ModeSetup` borrows `&ModeData` and `&MapData`; `ModeBook::new` keeps only what it resolves, so
  no match clones the mode's data.

### 8. The load check: the `ctx` convention enforced

In `runner/src/script_facts.rs`, the walk also records every use of a variable named `ctx` that is
neither the left side of `ctx.<name>` nor a whole argument of a call. The check refuses such a
use, and:

- every hook's first parameter is named `ctx`;
- where a script calls one of its own functions with `ctx` as an argument, that function's
  parameter at the same place is named `ctx`;
- no `let` binds a new `ctx`.

Then every value of `ctx` in a script is a variable named `ctx`, and the facts see all its uses.

## Order of work

Each step builds, passes the workspace check chain
(`cargo fmt && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test
--workspace --tests --all-features`), updates the design where it changes a rule, deletes its
items from `.notes/review.md`, and stops for review. The steps are in order of dependency, then of
risk.

1. **Small single owners.** `Fingerprint` into `protocol`; `Command::payload`;
   `Position::within_ground`; `SimTick::start` and `end` with `Timers::due(time)`.
   Tests: the session id and the log layout test with a `Fingerprint`; `Command::payload`'s bytes
   as the command test spells them; a reach just inside, on and just outside the radius at a
   height; a timer set at the start, in Inputs and in Mode fires at the same ends as now.

2. **Lanes, names and kits.** `OnLane` for every lane unit; unit type names in `UnitTypes`;
   `unit.unit_type`; kits only by type.
   Tests: the navigation state test with one lane component; a creep and a structure read the
   same `unit.lane`; `unit.unit_type` of a spawned creep; a wave's creeps spawn with the kit of
   their type.

3. **Data values and ranks.** `Param` over `Ranked`; ability number fields as `Ranked<Number>`;
   `AbilityData::check_ranks` as the one rank rule.
   Tests: a cooldown given as `{ param = "cd" }` resolves at ranks 1 and 5 to different ticks; a
   per-rank array of 4 in an ultimate fails with the rank count; the reference heroes still read
   and replay the same.

4. **Filters.** `Targeting::Unit(FilterData)`, resolved at load; `FilterSyntax` private.
   Tests: an ability targeting `enemies:hero` loads, casts at a hero, and refuses a creep; an
   unknown tag in targeting fails the load. This closes the first open issue.

5. **Typed manifest and the capability set.** `Speed`, `TickRange`, checked `ScriptLimits`,
   `CapabilitySet` with its needs and install; `Units` without scripts; the client from the
   declared set.
   Tests: each manifest flaw now fails with a read error; `CapabilitySet::new` refuses `mode`, a
   repeat and a missing need; the prototype test's client installs from the set.

6. **One script batch, a budget for each player.** `ScriptBatch`, `Pool`, per-slot budgets, the
   manifest's `player` pool; the three systems on the batch; design 02.
   Tests: player 0 sends mode inputs that spend its budget, and player 1's cast in the same tick
   still resolves; an AI unit and a timer that find their pool spent stay due, as now. This
   closes the second open issue.

7. **Match build.** `MatchBuild` apart from `ModePackages`; `ModeSetup` by reference.
   Tests: the 3v3 replay test and the verifier tests pass unchanged; `ModePackages` builds with no
   world.

8. **The `ctx` convention.** The walk records other uses; the three rules.
   Tests: `let c = ctx;`, a helper whose parameter receiving `ctx` has another name, a hook whose
   first parameter is not `ctx`, and `let ctx = 1;` each fail the load; every reference package
   still loads.

Steps 1 to 4 each fit in one session. Steps 5 and 6 are the largest: each changes the manifest
schema, so each also updates the manifests of the 3v3 and the test mode, and design 08's manifest
paragraph.
