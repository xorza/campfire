# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

**Slice 5, units that drive.** The user starts Tournament Desert in the client, finds America's command center and dozer at their start, selects the dozer, and drives it round the cliffs to the other start, at its speed and turn rate, on the drawn ground. The design is [Units that drive](docs/design/17-units-that-drive.md); the facts are [Zero Hour research](docs/research/zero-hour.md#movement). The Zero Hour check plays the drive, and loads every imported map.

**Slice 6, the issue log's ready items.** The user right-clicks the shrine with a hero and the hero uses it, and opens a night map and a snowy map of Zero Hour and sees their night and snow models; the file index reads through the codec.

**Before slice 7, the issue log's fixes.** The user opens Tournament Desert and sees no capsule where the original draws nothing, and its untextured triangles as the original draws them; the engine spells `center` one way.

**Slice 7, Zero Hour production.** The user starts Tournament Desert in the client with the skirmish's starting money, the command center trains a dozer, the dozer builds a barracks at a point the user chooses, and the barracks trains a ranger; each pays its cost and takes its build time. The design is [Zero Hour production](docs/design/18-zero-hour-production.md); the facts are [Zero Hour research](docs/research/zero-hour.md#production). The Zero Hour check plays the three, and the client's panel orders them.
