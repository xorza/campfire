# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](../AGENTS.md#workflow).

## Command panel

The slice of [Command panel](../docs/design/command-panel.md): a player of a Zero Hour skirmish gives commands from a panel of the game's own button images, laid out as the game lays them, presses each button by its key, and sells a building from it. Its facts are [Zero Hour's command panel](../docs/research/zero-hour-command-panel.md); its checks are that document's.

### CP1. The panel's data

Touches the client files; it stands first, as the import writes and the client reads what it adds.

- [ ] An action's look holds its effects alone (command-panel.md, A button's look is its action's; command-panel.md, An icon is a rectangle of a texture): `ActionLook` names `fire` and `hit` (`action_look.rs:10`). It gains `icon`, a texture of the package and its rectangle in pixels, and `key`, a key of the keys file's. Checks: the client files' tests read an icon and a key, and refuse an icon of an empty rectangle.
- [ ] A look places nothing on the panel (command-panel.md, A look places its buttons; command-panel.md, The panel's commands are the engine's orders): `ClientUnit` holds no panel (`client_unit.rs:20`), and the HUD file only its bars and hit mark (`hud_file/mod.rs:12`). A look gains `panel`, the place of each of its buttons, an action by its name or a command of `sell`, `stop`, `attack_move` and `rally`; the HUD file gains its panel's grid, its columns, rows and order of places, and each command's icon and key. Checks: the client files' tests read a look's places and the grid, and refuse a place past the grid or two buttons at one place.

### CP2. The game's buttons

Touches the Zero Hour import; it stands after CP1, whose data it writes.

- [ ] The import keeps a set's order and drops its places, its images and its keys (command-panel.md, A look places its buttons; command-panel.md, A key presses a shown button): `CommandSetIni::buttons` gives a set's buttons in order (`command_set_ini/mod.rs:42`), and `ProductionImport::text` drops a label's `&` marker (`production_import/mod.rs:703`). The import reads `MappedImages\`, writes each converted button's image as its action's icon on its atlas, converted as the models' textures are, and its label's marked letter as its key; each set's places 1 to 14 as its producer's look's places; `Command_Sell`, `Command_Stop`, `Command_AttackMove` and `Command_SetRallyPoint` as the commands at their places, their icons and keys the HUD file's; and a grid of 7 columns of 2, its places down each column. Checks: the import's tests convert a mapped image, a key, a set's places and its commands, and count a button of no image, a place past 14 and a button of no command the engine holds; the Zero Hour check pins the counts.

### CP3. A building's sale

Touches production and the Zero Hour import; it stands after CP2, beside whose import it writes a structure's refund.

- [ ] Nothing sells a building (command-panel.md, Selling is production's): production cancels a site and refunds a share of what its build paid (`production_orders.rs:122`), and holds no sale of one that stands. A unit type's `sell` gives its refund and its time; `ProductionDirective::Sell` sells a building its player controls: its train queue refunded at once, a part `Sold` of its start and its end, which every client that holds it receives, its orders refused, then at its end its refund paid and the unit removed with no death. The import writes each structure's `sell`, its `RefundValue` or its cost times `SellPercentage`, over 6 seconds. Checks: production's tests sell a building and pin its refund and its end, and refuse a sale of a site, of another player's building and of a sold one; the Zero Hour check's play sells a building and pins its refund and its tick.

### CP4. The panel of images

Touches the client; it stands after CP3, as its sell button sends the sale.

- [ ] The panel is rows of text (command-panel.md, A key presses a shown button; command-panel.md, A tooltip tells what an icon hides; command-panel.md, The panel's commands are the engine's orders): `CommandPanel` spawns a row for each button and queue entry (`command_panel/mod.rs:402`). The panel lays the shown unit's buttons at their places in the HUD file's grid, each its icon or its text, dimmed while its player cannot pay; hovering one shows its name, key, cost and time; a key with no modifier presses the shown button of that key; the queue shows each entry's icon, the head's progress a bar over it; a sale and a stop order at once, and an attack move and a rally take the point the next left click picks; a sold building leaves the selection and sinks as its sale runs. Checks: the client's tests lay a look's buttons at their places, press one by its key, show its tooltip, dim one its player cannot pay, and pick an attack move's point.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] A player of Tournament Desert trains and builds from the game's images, presses a button by its key, attack-moves and sets a rally from the panel, and sells a building, in the client; then the slice's decisions fold into `02-engine-core.md`, `04-capabilities/production.md` and `09-zero-hour.md`, and `command-panel.md` goes.

## Foundations
