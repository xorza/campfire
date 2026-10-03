# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Stage 3: Vertical slice, close

1. **Vertical slice playtest, fourth round**: the same 1v1 on LAN, with routes and collision. Units that block each other and body block are new to the feel; write down whether the match reads and plays well. This closes stage 3, or names what is missing.
2. **Collision bench**: on a quiet machine, run `cargo bench -p campfire-capabilities --features bench --bench collision`: 1000 bodies, crowded into 40 m square and spread over 120 m square, contacts found and parted. Check that it fits the Collide stage's share of a tick at 30 Hz, and record the numbers beside the collision design. A timing probe on a busy machine gave 0.14 to 0.42 ms, against 2.1 to 2.5 ms for a check of every pair.

## Stage 4: Game model

Touches: every capability, the registry, the package loader, the client, the reference packages. Design: [The model](docs/design/04-capabilities/00-overview.md#the-model), the capability docs, [Game package](docs/design/03-game-scripting.md#game-package). Each step rewrites the reference packages to the model as it goes, so the 3v3 match keeps running with the same rules; until its step, the code that differs from the design is the code's to fix.

Each step's shape moves into the design when the step lands. Each step ends with the check chain, both goldens and the structure tests ([Structural rules](docs/design/02-engine-core.md#structural-rules)); a step that changes behaviour names the change.

1. **S1: the 3v3 played by scripted players** ([Testing and diagnostics](docs/design/02-engine-core.md#testing-and-diagnostics)). Size M. Changes the 3v3 golden.
   - Today the 3v3 test sends only the picks. In its 3,200 ticks only creeps strike: 345 attacks of melee and caster creeps. No tower fires, no camp fights, no hero acts, no hero or structure dies, and the mode script's bounties, assists, experience shares and respawns never run. The reference scripts run only in that test, as the engine's tests keep copies of their own.
   - The players play a script of orders, as the proving match's do: each order stamped for a tick, its units named by role and resolved to stable ids when it is sent: the player's hero, the nearest enemy hero, the enemy creep of least life within a range, a lane's outer tower, a camp's unit. One driver of a stamped script serves both matches.
   - Lanes, the golden `3v3`: the heroes walk with the first waves and last-hit creeps, one casts haste and one mend, one dives the enemy outer tower and dies to it, and two kill an enemy hero. Each assert is computed from the content's numbers: a creep's bounty to the hero that killed it, its experience split among the heroes within `xp_radius`, first blood and the split of `assist_gold`, the tower's switch to the hero that hit an allied hero, and the dead hero's respawn tick from `respawn_base_ms`, `respawn_per_level_ms` and its level.
   - A camp, the golden `3v3-camp`: a hero attacks the wolves; they chase it; it walks past `leash_range`, and they reset to their camp; it comes back and kills one: its bounty and experience, and its respawn after `camp_respawn_ms`.
   - The late rules, which no short match reaches at level 1, get a rules test on the 3v3's packages: an inhibitor's fall and its respawn after `inhibitor_respawn_ms`, the super creeps of the next wave on that lane, the warden's blessing to each hero of the killer's team, and the core's fall, which ends the match for the other team. It deals the damage through the runner's `world_mut`, as no input can, and asserts each rule's result; it has no golden and no replay, as the log does not hold that damage.
   - Each test runs within 1 s with its replay; a scenario that does not fit is split, each part with its own golden.
   - The heroes cast their spells alone: no input gives a hero a rank until stage 5's `learn` order, and several of their abilities need stage 5's mechanics. The scenario plays their abilities in stage 5; until then, `reference_abilities` tests each ability alone.
   - It closes stage 4: its done condition counts on no failed call of the mode, creep, tower and camp scripts, which only this play makes fight.
