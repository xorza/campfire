# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](../AGENTS.md#workflow).

## Power

The slice of [Power](../docs/design/power.md): a player of a Zero Hour skirmish as America builds past its power and sees the bar turn red, its Patriot batteries go dark and silent, its minimap go out and its trains and builds slow, then builds a power plant or buys the Control Rods and sees all of it come back; as GLA it trains a Rebel in the original's 10 s. Its facts are [Zero Hour's power](../docs/research/zero-hour-power.md); its checks are that document's.

### Batch 3: the import, the rules and the bot

It touches the import, the rules package and its bot, and stands after the engine holds power.

- [ ] **PW5. The import gives power.** The import reads no `EnergyProduction`, `EnergyBonus`, `KindOf = POWERED`, `DisableProof` or `GameData.ini`'s low-energy and power-bar fields (`ini_build.rs:89`, `object_kinds.rs:26`, `game_data_ini/mod.rs:14`), and converts a `PowerPlantUpgrade` to a tag alone and a `RadarUpgrade` with no conflict (`upgrade_import/mod.rs:244`, `:245`). It writes each type's power stat, the tag `powered`, the Control Rods' `power_plant_upgraded_<n>` modifiers, each radar upgrade's conflict but the Radar Van's, the mode's `[power]` and the HUD's power bar and mark; the rules package declares the stat `power`, the tags `powered` and `disabled` with its blocks, and the modifier `underpowered` (Direction: the importer takes a game's data as it is; a capability serves every game). Check: the design's import tests; the Zero Hour check's golden moves only by the trains' and builds' times and the power play.
- [ ] **PW6. A bot builds power while short.** The bot builds the first base building it lacks (`bot.rhai:87`). While its player's `power_used` passes its `power_given` and none of its faction's `power_plants_<faction>` is under way, it builds one first. Check: the Zero Hour check's bases play keeps America's and China's bots never short past the time one power plant takes, measured against today's bases.

### Batch 4: the client

It touches the client's HUD, panel and looks, and stands last but the player's check, as it shows what the engine now counts.

- [ ] **PW7. The panel's power bar.** The panel shows the player's resources alone (`command_panel/mod.rs:99`). The HUD file's `power_bar` (`hud_file/mod.rs:18`) gives its images, base, intervals and margin, and the panel draws the received `PlayerPower` as its bar and needle in green, yellow or red. Check: a client test of the bar's length, needle and color at given counts, computed by hand.
- [ ] **PW8. A tag marks its unit.** A unit's paint takes only the build preview's tint (`match_colors.rs:23`), no tag shows an icon over the bars but the level's (`hud/mod.rs:924`), and nothing stills a look's clips (`look_clips.rs:64`). The HUD file's `tag_marks` give a tag's tint faded over its time, its looping icon over the bars, and whether it stills animations, which a look may keep (Direction: one rendering pipeline for every game). Check: client tests of a marked unit's tint at points of its fade, its icon's frame at given times and its clips stilled, and of a look that keeps them.

### End of the slice

- [ ] A player of Tournament Desert as America against the China builder bot, then as GLA, does what the design's last check says, in the client; then the slice's decisions fold into `04-capabilities/production.md`, `02-engine-core.md`, `03-game-scripting.md` and `09-zero-hour.md`, its losses into `09-zero-hour.md`'s in place of the Power loss, and `power.md` goes.

## Foundations

## Changed

- **PW2. A shortage holds its modifier.** The count sets the modifier to the player's shortage as each tick ends, whatever the player held, in place of a restore check of it: `PlayerModifiers`' check is the stats crate's, below production, and a level-triggered count makes a restored or a script-changed set hold as the count does from the next tick's end, which the runner test shows.

## Decisions

- **Power is sent under supply's kind.** `PlayerPower` replicates as `DataKind::Supply`, a player's counts of production, as a kind of its own passed Replicon's 32 visibility scopes (`bevy_replicon` 0.45, `visibility/registry.rs:61`), which every server test then failed on.
- **A speed names the kinds it slows.** `slows` is a field of `[power] speed`, not of `[power]`, so no mode names kinds to slow with no speed to slow them by.
- **The count is `PowerCount`'s.** The refresh is an associated function of a unit type of its own module, as `PlayerPower`, `Holdings` and `PowerSpeed` read one another otherwise, which the rules' test of module cycles refuses.
- **The holdings mark a unit whose stats changed.** In a mode that counts power, each follow scans the owned units' `UnitStats` change ticks, as it scans the queues', since a stat changes in place with no part inserted or removed; a unit's stats empty before their first refresh count 0 until it marks the unit.
