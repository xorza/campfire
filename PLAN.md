# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

**Slice 5, units that drive.** The user starts Tournament Desert in the client, finds America's command center and dozer at their start, selects the dozer, and drives it round the cliffs to the other start, at its speed and turn rate, on the drawn ground. The design is [Units that drive](docs/design/17-units-that-drive.md); the facts are [Zero Hour research](docs/research/zero-hour.md#movement). The Zero Hour check plays the drive, and loads every imported map.

**Slice 6, the issue log's ready items.** The user right-clicks the shrine with a hero and the hero uses it, and opens a night map and a snowy map of Zero Hour and sees their night and snow models; the file index reads through the codec.

**Before slice 7, the issue log's fixes.** The user opens Tournament Desert and sees no capsule where the original draws nothing, and its untextured triangles as the original draws them; the engine spells `center` one way.

**Slice 7, Zero Hour production.** The user starts Tournament Desert in the client with the skirmish's starting money, the command center trains a dozer, the dozer builds a barracks at a point the user chooses, and the barracks trains a ranger; each pays its cost and takes its build time. The design is [Zero Hour production](docs/design/18-zero-hour-production.md); the facts are [Zero Hour research](docs/research/zero-hour.md#production). The Zero Hour check plays the three, and the client's panel orders them.

**Optimization, in a session the user starts.** The change-driven passes wait in [optimization](.notes/optimization.md), with what the first session measured.

## 1. Zero Hour's units drive

`import`: `zero_hour`; `source/packages/zero-hour`; `checks/zero-hour-check`. It needs the turn rate, the load's rules and the selection to play.

### U9. The Zero Hour check compares fingerprints

(`checks/zero-hour-check`; Zero Hour D4, D7.) The Zero Hour check compares the fingerprint of its import with the one the other OS gave, as design 12 D7 says. Check: a run that names no install fails; a run on Linux and one on macOS give one fingerprint.

## 2. The command panel

`client`: a new `command_panel`, `orders`, `pointer`, `view`. It needs P4 to be played.

### P6. The client's command panel and the build preview

(`client`; design 18, D7.) The panel of the first selected producer, builder or site: its buttons with names, costs and whether they are affordable; the queue with the head's progress and a cancel by click; a site's progress and cancel; the player's money. A build button shows the building's look at the cursor, red over a wall or a held body, placed by a left click and dropped by a right click or Esc. A right click with producers selected and no walker sets their rally point. Check: the panel's rows, the preview's colour and the right click's order, each against a fixture, as design 18's checks give them; the user plays the slice.
