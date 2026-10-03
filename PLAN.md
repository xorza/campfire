# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Stage 3: Vertical slice, close

1. **Vertical slice playtest, fourth round**: the same 1v1 on LAN, with routes and collision. Units that block each other and body block are new to the feel; write down whether the match reads and plays well. This closes stage 3, or names what is missing.
2. **Collision bench**: on a quiet machine, run `cargo bench -p campfire-capabilities --features bench --bench collision`: 1000 bodies, crowded into 40 m square and spread over 120 m square, contacts found and parted. Check that it fits the Collide stage's share of a tick at 30 Hz, and record the numbers beside the collision design. A timing probe on a busy machine gave 0.14 to 0.42 ms, against 2.1 to 2.5 ms for a check of every pair.

## Stage 4: Game model

Touches: every capability, the registry, the package loader, the client, the reference packages. Design: [The model](docs/design/04-capabilities/00-overview.md#the-model), the capability docs, [Game package](docs/design/03-game-scripting.md#game-package). Each step rewrites the reference packages to the model as it goes, so the 3v3 match keeps running with the same rules; until its step, the code that differs from the design is the code's to fix.

Each step's shape moves into the design when the step lands. Each step ends with the check chain, both goldens and the structure tests ([Structural rules](docs/design/02-engine-core.md#structural-rules)); a step that changes behaviour names the change.

1. **M4: a delivery's hits and sight** ([Deliveries](docs/design/04-capabilities/actions.md#deliveries)). Size S. Changes the package API: `collide` and `sight_radius` go.
   - Today the load takes `collide` and `sight_radius` as planned fields and ignores them, and refuses a `vision` section on a delivery type. Snow Owl, `collide = false`, finds hits on its way that do nothing, and reveals nothing as it flies.
   - A projectile's `hits` takes `none`: it hits no unit and flies to its end. The load refuses `none` beside `width`, `stop_on_hit`, `once_per_cast` or `homing`, and for an action with an `on_hit` list or hook.
   - A projectile or an area type takes a `vision` section; the delivery spawner gives its unit its `Sight`, so it reveals to its source's group while it lasts.
   - Snow Owl becomes `projectile = { speed = "14", hits = "none" }` with `vision = { sight_range = "4.0" }`. The 3v3 golden does not change if no bot casts it; the step says whether it does.
   - Tests: a projectile with `hits = "none"` crosses an enemy's body and ends at its range, no hit recorded; each refusal of `none`; a projectile with a vision section reveals its cells to its source's group in each tick it flies and none after it ends, and an area's the same while it lasts.
2. **M5: the `launch` effect** ([Effects](docs/design/04-capabilities/actions.md#effects)). Size M. Changes the package API: an effect the load refused, it now takes.
   - Today the load refuses `launch` as planned.
   - `launch = { area, on_hit, on_end }`: an area type of the action's package lands where the unit reached stands, or the acting unit with `to = "source"`, from the acting unit, with the action's params at its rank. Its nested lists run on the units it reaches and at its end; it runs no hook. The lists stay one flat buffer, a launch holding the ranges of its own lists, so a nested list costs no allocation of its own.
   - The step states when a launched area lands: one launched in Hit by a delivery's list, and one launched in Resolve by a weapon's `on_hit`. It writes that into the design, as an area `ctx.area` queues at the same point.
   - Tests: a weapon whose `on_hit` launches a splash of radius 2 that deals half its attack damage: the target takes the attack and the splash, a unit 1.5 m off takes the splash alone, one 3 m off takes nothing, with exact lives; a nested launch two levels deep runs both lists once; `to = "source"` lands it at the attacker; the load refuses a `launch` of a projectile type, and of a type of another package.
