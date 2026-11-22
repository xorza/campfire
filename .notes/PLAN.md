# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](../AGENTS.md#workflow).

## Power

The slice of [Power](../docs/design/power.md): a player of a Zero Hour skirmish as America builds past its power and sees the bar turn red, its Patriot batteries go dark and silent, its minimap go out and its trains and builds slow, then builds a power plant or buys the Control Rods and sees all of it come back; as GLA it trains a Rebel in the original's 10 s. Its facts are [Zero Hour's power](../docs/research/zero-hour-power.md); its checks are that document's.

### End of the slice

- [ ] A player of Tournament Desert as America against the China builder bot, then as GLA, does what the design's last check says, in the client; then the slice's decisions fold into `04-capabilities/production.md`, `02-engine-core.md`, `03-game-scripting.md` and `09-zero-hour.md`, its losses into `09-zero-hour.md`'s in place of the Power loss, and `power.md` goes.

## Foundations

## Changed

- **PW2. A shortage holds its modifier.** The count sets the modifier to the player's shortage as each tick ends, whatever the player held, in place of a restore check of it: `PlayerModifiers`' check is the stats crate's, below production, and a level-triggered count makes a restored or a script-changed set hold as the count does from the next tick's end, which the runner test shows.

- **PW5. The import gives power.** The shortage's modifier is the import's, as a rules package holds no modifier (`LoadProblem::RulesKey`), and so is `[power] short`; the rules package keeps the stat and the tags. A radar its object grants itself as it is made is an upgrade's entry of no trigger, in place of a tag of its type, so its conflict with the shortage reaches it. The Control Rods' modifiers are `<trigger>_power_<bonus>`, after the import's names of an upgrade's effects.
- **PW6. A bot builds power while short.** The Zero Hour check plays power in a play of its own, a Patriot battery disabled by a second one past one plant's power and enabled by a second plant, as a Patriot needs a plant; the skirmish's builder fixture waits the 300 ticks more its first plant takes, as its timed orders took the dozer off a plant half built.

- **PW7. The panel's power bar.** The bar stands at the window's top, its own node, not the command panel's, which shows only while a unit is selected, where Zero Hour's control bar always draws its bar; it shows a player's power once the server sends it, a GLA player's empty.

- **PW8. A tag marks its unit.** A mark tints and stills its unit, but draws no icon over its bars: Zero Hour's `Disabled` icon is an `Animation2D.ini` animation, a format the import does not read, and a looping billboard the HUD lacks, depth the slice does not need, as the tint and the stilled clips show the state; the icon is in the issue log. The tint's 30 steps are colors of the one buffer, as a mesh's tag names its color by place.

## Decisions

- **Power is sent under supply's kind.** `PlayerPower` replicates as `DataKind::Supply`, a player's counts of production, as a kind of its own passed Replicon's 32 visibility scopes (`bevy_replicon` 0.45, `visibility/registry.rs:61`), which every server test then failed on.
- **A speed names the kinds it slows.** `slows` is a field of `[power] speed`, not of `[power]`, so no mode names kinds to slow with no speed to slow them by.
- **The count is `PowerCount`'s.** The refresh is an associated function of a unit type of its own module, as `PlayerPower`, `Holdings` and `PowerSpeed` read one another otherwise, which the rules' test of module cycles refuses.
- **A speed of none is the original's floor.** The import writes a `min` or a `max` of 0 as 0.01, `calcTimeToBuild`'s rate of a rate of none, and a `min` past its `max` as the `max`, which every short player then takes in the original too; a `min` of 0 differs only for a rate between 0 and 0.01, which the original takes as it is.
- **The holdings mark a unit whose stats changed.** In a mode that counts power, each follow scans the owned units' `UnitStats` change ticks, as it scans the queues', since a stat changes in place with no part inserted or removed; a unit's stats empty before their first refresh count 0 until it marks the unit.
