# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](../AGENTS.md#workflow).

## Power

The slice of [Power](../docs/design/power.md): a player of a Zero Hour skirmish as America builds past its power and sees the bar turn red, its Patriot batteries go dark and silent, its minimap go out and its trains and builds slow, then builds a power plant or buys the Control Rods and sees all of it come back; as GLA it trains a Rebel in the original's 10 s. Its facts are [Zero Hour's power](../docs/research/zero-hour-power.md); its checks are that document's.

### Batch 1: the count and the shortage

It touches `production`, `stats`' player modifiers, the script API and the replication of the player's entity, and stands first, as every later step reads the count.

- [ ] **PW1. The mode's power and a unit's stat.** Production has no power: the holdings count each unit's supply and type alone (`holdings.rs:31`, `Share` at `:46`). The mode's `[power]` names its stat, its shortage's modifier, its speed and the kinds it slows, checked at the load; the holdings count each player's given and used power from the stat of each living, owned, complete unit, riders and sold buildings among them, and count a unit again as its stats change; the count runs as each tick ends on the server and writes `PlayerPower` on the player's entity, sent on change to its audience as `PlayerSupply` is (`player_supply.rs:36`, `lib.rs:344`), with `power_given` and `power_used` beside `supply_used` (`production_api.rs:96`) (Direction: a module adds what it declares once; a pass visits what changed). Check: the design's production tests of the count.
- [ ] **PW2. A shortage holds its modifier.** No count changes a player's modifiers but research and perks (`player_modifiers.rs:17`). The count adds the mode's shortage modifier to a player's modifiers as it falls short and removes it as power returns, so `PlayerGrants::want` (`player_grants.rs:53`) holds it on the units it affects from the next tick, and `PlayerModifiers`' check (`player_modifiers.rs:52`) holds it equal to the count after a restore (Direction: what can be forgotten is a type). Check: the design's production tests of the shortage and the restore.

### Batch 2: the speed

It touches `production`'s trains and construction, and stands after the count, which gives the speed.

- [ ] **PW3. A train's end follows the speed.** A queue's head ends at a tick fixed as it reaches the head (`train_queue.rs:102`, `:124`). The head keeps the tick it reached the head and ends at that tick plus its time at its player's speed, for the kinds `[power] slows` lists; the count moves each queue's head of a player whose speed changed in the index of the queues, and `Trains::finish_trains` (`trains.rs:268`) ends a head past its new end (Direction: a pass visits what changed). Check: the design's production tests of trains, at the ticks they compute; the snapshot's data version rises with the queue's layout.
- [ ] **PW4. A site grows at the speed.** `Construction::progress_sites` (`construction/mod.rs:315`) grows a site by its builders' rate alone (`site.rs:125`). Its progress each tick takes the rate times its time over its time at its player's speed, on the server and on a predicting client from the count it receives. Check: the design's production tests of sites.

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
