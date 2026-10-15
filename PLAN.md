# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

**Slice 5, units that drive.** The user starts Tournament Desert in the client, finds America's command center and dozer at their start, selects the dozer, and drives it round the cliffs to the other start, at its speed and turn rate, on the drawn ground. The design is [Units that drive](docs/design/17-units-that-drive.md); the facts are [Zero Hour research](docs/research/zero-hour.md#movement). The Zero Hour check plays the drive, and loads every imported map.

**Slice 6, the issue log's ready items.** The user right-clicks the shrine with a hero and the hero uses it, and opens a night map and a snowy map of Zero Hour and sees their night and snow models; the file index reads through the codec.

**Before slice 7, the issue log's fixes.** The user opens Tournament Desert and sees no capsule where the original draws nothing, and its untextured triangles as the original draws them; a model with nothing to fit stops being walked each frame; a blessing run writes each golden once; the engine spells `center` one way.

**Slice 7, Zero Hour production.** The user starts Tournament Desert in the client with the skirmish's starting money, the command center trains a dozer, the dozer builds a barracks at a point the user chooses, and the barracks trains a ranger; each pays its cost and takes its build time. The design is [Zero Hour production](docs/design/18-zero-hour-production.md); the facts are [Zero Hour research](docs/research/zero-hour.md#production). The Zero Hour check plays the three, and the client's panel orders them.

**Optimization, in a session the user starts.** The change-driven passes wait in [optimization](.notes/optimization.md), with what the first session measured.

## 1. Zero Hour's units drive

`import`: `zero_hour`; `source/packages/zero-hour`; `checks/zero-hour-check`. It needs the turn rate, the load's rules and the selection to play.

### U9. The Zero Hour check compares fingerprints

(`checks/zero-hour-check`; Zero Hour D4, D7.) The Zero Hour check compares the fingerprint of its import with the one the other OS gave, as design 12 D7 says. Check: a run that names no install fails; a run on Linux and one on macOS give one fingerprint.

## 2. The issue log's fixes

`client`: `view`; `harness`: `golden`, `match_record`; `import`: `zero_hour`. It stands before production, which the user plays in the same client.

### I1. A model drawing stops fitting a model that gives no bounds

(`client`: `view`; the issue log's Client item.) `View::fit_models` keeps `FitsModel` while a model's meshes wait for their bounds, and ends the fit, keeping the shape's height, once the model's scene has spawned and no node it leaves shown holds a mesh, or once the model's file fails to load. Check: a fixture model whose look hides every node, and one whose file does not load, lose `FitsModel` at the first run past their load; one whose meshes wait for bounds keeps it.

### I2. One test writes each golden

(`harness`: `golden`, `match_record`; `runner/proving_match.rs`; the issue log's Harness item.) `MatchRecord` gains a comparison with a golden that never writes it, and the reverse query order test compares with `golden/proving.txt` by it, so under `--bless` only the proving match writes the file. Check: under `CAMPFIRE_BLESS=1`, the comparison leaves a fixture golden as it was, and fails when it differs.

### I3. An untextured triangle samples the null vector

(`import`: `zero_hour::model_import`; design 12's Models; the issue log's Zero Hour item of no texture.) A triangle of no texture, under a shader that textures, takes its texture's color and alpha as 0, as native Direct3D samples a stage of no texture, so its material is what its shader makes of the null vector: a lit shader's modulation black, of alpha 0, which an opaque shader draws black, an alpha test discards, and a blend by alpha draws as nothing. Design 12 and the research name the rule. Check: a fixture mesh's untextured triangle under an opaque lit shader, an alpha test and a blend gives the material computed by hand for each.

### I4. An object that draws nothing has an empty look

(`import`: `zero_hour`; `client`: `view`; design 12's Units; the issue log's Zero Hour item of objects that draw nothing.) An object whose draw modules name no model, as a `W3DDefaultDraw` of none, gets a look of no models in `client/units.toml`; one whose model the package lacks keeps none, so it shows as the engine's shape and the import reports the model. The client draws nothing for a unit of a look of no models, and the engine's shape for a type of no look. Check: the fixture's sound object has an empty look; the client spawns no drawing for a unit of an empty look and a capsule for one of no look; the Zero Hour check pins the count of empty looks.

## 3. One spelling of `center`

Every crate, `docs/`, `AGENTS.md`. A rename alone, so it stands apart from the fixes.

### I5. The engine spells the middle of a shape `center`

(Every crate's names and comments, and the documents; the issue log's Geometry item.) Each `centre` of the engine's names, comments and documents becomes `center`, Bevy's spelling, so one word has one spelling. Check: no `centre` is left outside quoted sources, and the goldens do not move.

## 4. Production's start and exit

`production`; `docs/design/04-capabilities/production.md`. It stands before batch 5, as the import writes what it adds.

### P1. A build may start as it is ordered

(`production`: `construction`, `build_book`, `builder`; design 18, D1.) A `build` action gains `start`, `"arrival"` by default or `"order"`. An order build starts in Act of the tick its order applies, wherever its builder stands: it passes its checks, pays, goes on cooldown, spawns its site, and turns its order to the site, which its builder walks to and builds. A start that fails ends the order and pays nothing. [Construction](docs/design/04-capabilities/production.md#construction) and its Data gain the rule. Check: an order build far from its builder pays and spawns its site in the tick it applies, and its builder walks to it and completes it; one whose placement fails ends and pays nothing; an arrival build is unchanged, and the goldens do not move.

### P2. A producer names its exit and its default rally point

(`production`: `production_data`, `trains`; design 18, D2.) `production` gains `exit` and `rally`, `[x, z]` in the producer's frame, turned with it. A trained unit spawns at the free cell nearest the exit point; with no rally point set it walks to the default one. [Train queues](docs/design/04-capabilities/production.md#train-queues) and its Data gain the rule. Check: a producer turned 90° spawns its unit at the cell nearest its turned exit, computed by hand, which walks to the turned default rally point, and to the player's rally point once one is set; a producer of neither is unchanged.

## 5. Zero Hour's trains and builds

`import`: `zero_hour`; `source/packages/zero-hour`; `checks/zero-hour-check`; `actions`'s load. It needs P1 and P2.

### P3. The load names every train and build

(`actions`: the action load; design 18, D6.) The load refuses a `train` or `build` action with no `name`, as it refuses an avatar's action with none; the proving mode's `train_guard` gains its name. Check: a train of no name is refused, with its action named.

### P4. The import converts the command sets, the money and the names

(`import`: `zero_hour`; the rules package's mode script; design 18, D3 to D6.) The import reads `CommandSet.ini`, `CommandButton.ini`, `multiplayer.ini` and the string table; makes each `UNIT_BUILD` and `DOZER_CONSTRUCT` button an action, each object a converted button makes a unit type, with its slots, its queue, its exit and its rally; drops and reports the buttons D3 names; writes the mode's `production`, `money`, `command` slot kind, `start_money` and the actions' names. The mode script gives each player its money. `imported_type.rs`'s comment whose line runs past 100 characters is wrapped. Check: the fixtures of D3 to D6 as design 18's checks give them; the rules package's script gives a fixture match its money.

### P5. The Zero Hour check plays production

(`checks/zero-hour-check`; design 18's checks.) The check pins the counts of trains, builds and dropped buttons by cause, and plays on Tournament Desert the command center's dozer, the dozer's barracks and the barracks' ranger, each paid from 10,000 and done in its ticks. Check: `./drive.py zero-hour` passes.

## 6. The command panel

`client`: a new `command_panel`, `orders`, `pointer`, `view`. It needs P4 to be played.

### P6. The client's command panel and the build preview

(`client`; design 18, D7.) The panel of the first selected producer, builder or site: its buttons with names, costs and whether they are affordable; the queue with the head's progress and a cancel by click; a site's progress and cancel; the player's money. A build button shows the building's look at the cursor, red over a wall or a held body, placed by a left click and dropped by a right click or Esc. A right click with producers selected and no walker sets their rally point. Check: the panel's rows, the preview's colour and the right click's order, each against a fixture, as design 18's checks give them; the user plays the slice.
