# Zero Hour's command panel

How Zero Hour lays out its command panel, draws and keys its buttons, and sells a building, how OpenRA does the same, and what the engine holds of it, read on 27 October 2026 from [GeneralsX](https://github.com/fbraz3/GeneralsX) (`ControlBar.cpp`, `ControlBarCommand.cpp`, `HotKey.cpp`, `BuildAssistant.cpp`), the install's `INIZH.big` (`CommandSet.ini`, `CommandButton.ini`, `MappedImages\`, `GameData.ini`), `WindowZH.big` (`ControlBar.wnd`) and `EnglishZH.big`, OpenRA's `ProductionPaletteWidget.cs` and `Sellable.cs`, and the engine's code.

## The panel

- **A command set is up to 18 places.** `CommandSet.ini` gives each set a button at each of its places, `1` to `18`; the control bar draws 14, and places 15 to 18 are for scripts alone (`ControlBar.h`, `MAX_COMMANDS_PER_SET`). Of 413 sets, places 1 to 14 hold 3,101 buttons, and place 15 holds 7.
- **Places run down each column.** `ControlBar.wnd` lays its 14 command windows in 7 columns of 2 at a resolution of 800 by 600: place 1 at (223, 494) to (273, 538), place 2 under it at (223, 545), place 3 at the top of the next column, 55 pixels right; each button is 50 by 44 pixels.
- **The last places hold the common commands.** Of the sets, place 14 holds `Command_Sell` in 209 and `Command_Stop` in 177, place 13 `Command_Guard` in 147 and `Command_SetRallyPoint` in 105, and place 11 `Command_AttackMove` in 137.
- **A button's command.** Of 817 buttons, 259 are `UNIT_BUILD`, 192 `DOZER_CONSTRUCT`, 83 `SPECIAL_POWER`, 82 `PURCHASE_SCIENCE`, 58 `PLAYER_UPGRADE`, 47 `SPECIAL_POWER_FROM_SHORTCUT`, 37 `OBJECT_UPGRADE`, 15 `FIRE_WEAPON` and 9 `SWITCH_WEAPON`; `SELL`, `STOP`, `SET_RALLY_POINT`, `ATTACK_MOVE` and `GUARD` are one button each.

## A button's image and key

- **An image is a rectangle of an atlas.** A button's `ButtonImage` names a `MappedImage` of `Data\INI\MappedImages\`, a `Texture`, a `.tga` of 512 by 512, and its `Coords`, as `Left:63 Top:301 Right:123 Bottom:349` for the dozer's `SACDozer`, 60 by 48 pixels. The atlases are the language's art, `Data\English\Art\Textures\` of `EnglishZH.big`. Of 283 images the buttons name, 2 have no mapped image.
- **A button draws its image alone.** `ControlBar::setControlCommand` sets the button's enabled image to its `ButtonImage`; its text, `TextLabel`, shows in its tooltip, with its `DescriptLabel`.
- **A key is its label's marked letter.** `HotKeyManager::searchHotKey` takes the character after the first `&` of the button's `TextLabel` in the string table, as `&Dozer`'s `d`; a key released with no modifier presses the shown, enabled button of that key, and a disabled one plays a click alone (`HotKeyTranslator::translateGameMessage`, `executeHotKey`).
- **The queue draws a clock.** A trained unit's queue entry shows its button's image under a clock that fills as it builds (`GadgetButtonDrawClock`).

## Selling

- **A sale runs construction backwards.** `BuildAssistant::sellObject` sets a structure's construction percent to 99.9, marks it `SOLD` and unselectable, deselects it, refunds its production queue, and makes it idle; it stays a target. After the scaffold's 1.5 seconds, `FRAMES_TO_ALLOW_SCAFFOLD`, its percent falls each frame by 100 over 3 seconds, `TOTAL_FRAMES_TO_SELL_OBJECT`; at 0 its model takes the `SOLD` condition, and at −50, 1.5 seconds later, 6 seconds after the sale, its player gets its refund and it is destroyed with no death (`BuildAssistant::update`).
- **The refund is a share of its cost.** A template's `RefundValue`, or else its cost to build times `GameData.ini`'s `SellPercentage`, 50 %.

## OpenRA

- **A palette keys by place.** `ProductionPaletteWidget` gives each shown icon the hotkey of its place, `HotkeyPrefix` and its number up to `HotkeyCount`, so a key presses a different item as the list changes; it lays its icons in `Columns` of `IconSize`, draws a clock of `ClockSequence` frames over the head of the queue, and darkens an item it cannot build.
- **A click queues, holds or cancels.** A left click starts an item, Shift five of it; a right click holds an item that started, and cancels one that did not; a middle click cancels.
- **Selling is a trait and an order.** `Sellable` refunds `RefundPercent`, 50 by default, of an actor's value times its share of health; its `Sell` order plays its make animation backwards, then its `Sell` activity refunds and removes it.

## What the engine holds

- **The panel is text.** The client's command panel lists the first selected unit's train, build and research actions as rows of text, its name, cost and time, dimmed while its player cannot pay, then its queue as rows, a click on one cancelling it (`command_panel/mod.rs`).
- **The import keeps a set's order, not its places.** `CommandSetIni::buttons` gives a set's buttons in the order of their places, and drops the places; a button's text is its label's, its `&` marker dropped (`production_import/mod.rs`, `ProductionImport::text`).
- **No sale.** Production cancels a site and refunds a share of what its build paid, and has no sale of a building that stands.
- **No image in the client's UI.** The client draws no image node; its panel, its HUD and its shop are text.
