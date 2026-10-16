# Match options

The player starts a Zero Hour skirmish with 50,000 money for every player, as the original's lobby offers it, against a bot that builds a base and trains units but never attacks. It builds on [Sessions](10-sessions.md), the [Protocol](05-protocol-spec.md#keys), [Game scripting](03-game-scripting.md) and [Zero Hour](12-zero-hour.md). The practice it follows is in [Practice](#practice).

## Decisions

- **A mode declares its options.** A mode's data takes `[options.<name>] = { choices = [<number>, ...], default = <number> }`: a param of the mode whose value the session's host picks among the choices, as OpenRA's `PlayerResources` declares the lobby option `startingcash` of its `SelectableCash`, and Zero Hour's `Multiplayer.ini` lists its `MultiplayerStartingMoneyChoice`s. A free value is refused: a list of choices lets the load check what a session may set, and the original offers no other. An option is a number, as the one option this slice needs is; a choice of text is planned. The load refuses an option of no choice, of more than 256, of one choice twice, whose default is no choice, or of the name of a param the mode declares too.
- **The host picks, and the terms record the pick.** A session's terms hold, for each option the mode declares, in the order of their names, the index of its choice: `u64 option count ‖ u8 choice index per option` in the session id after the visibility, its domain `campfire/session-id/v4`, as its layout changes and an id of the old layout must name no session of the new, as Zero Hour's game options string carries its starting cash, `SC=`, to every player and into its replay's header, and OpenRA's lobby settings hold each option's value. The terms carry the index, not the value, as the mode's fingerprint fixes the values; a session's rules refuse terms of another count of options or an index past an option's choices, as they refuse a visibility the mode does not admit.
- **A script reads the pick as the param.** `ctx.p.<name>` gives the chosen value, the default with no pick, so a mode's scripts are the same whether a param is an option or not. The match takes the picks as its mode installs, before `on_match_start`, and a restore target takes them from the terms it restores, so a replay, a verifier and a loaded save play with the session's own picks.
- **A pick is given by its value.** The server's and the local client's command line take `--option <name>=<value>`, a value of the option's choices; a name the mode does not declare, or a value of no choice, is refused before the session opens. An option not named takes its default.
- **The import writes Zero Hour's money as an option.** `start_money` becomes the option of the choices of `Multiplayer.ini`, in their order, its default the choice marked `Default`, in place of the param it was.
- **A bot that builds and does not fight.** An order script for the bot's slot on Tournament Desert has its dozers build a power plant, a barracks, a supply center and a war factory, and its buildings train dozers, infantry and vehicles, and gives no attack and no move toward another player. The Zero Hour check plays it as its bot and pins what the bot built and trained, and that no unit of the bot stands farther from its start than its farthest building's site, so the script keeps working.

## Practice

What established games do, read on 16 October 2026:

- **OpenRA.** `PlayerResourcesInfo` holds `SelectableCash` and `DefaultCash`; its `LobbyOptions` yields the option `startingcash` of those values, the default always among them; the host sets it in the lobby, the session's global settings hold it, and `PlayerResources` reads it by `OptionOrDefault("startingcash", ...)` as the match starts.
- **Zero Hour.** `Multiplayer.ini` lists 5,000, 10,000, 20,000 and 50,000, the second `Default = Yes`; the host picks one, `GameInfo` holds it, `GameInfoToAsciiString` writes it as `SC=` into the options string every player receives, and the recorder writes that string into the replay's header.

## Data

- **Mode** (`data/mode.toml`): `[options.start_money] choices = [5000, 10000, 20000, 50000]`, `default = 10000`, in place of `[params] start_money`.
- **Terms**: the choice index of each option.
- **Bot** (an order script beside the Zero Hour check): the builder bot's orders.

## Checks

- Package: an option reads with its choices and its default; each refusal of the load, by its error.
- Sessions: terms of each pick give their own session id; the rules refuse another count of options and an index past the choices; a match of a pick reads its value as `ctx.p`, one of none the default; a restore target of terms plays with their picks; a command line's `--option` of a value of no choice, and of a name the mode lacks, is refused.
- Import: the fixture's money choices become the option, the default its `Default`.
- Zero Hour check: a skirmish of `start_money` 50,000 starts each player with it; the bot builds its buildings and trains its units, each pinned, and no unit of it stands farther from its start than its farthest site. Its fingerprints are pinned again from a run on Linux and a run on macOS.
