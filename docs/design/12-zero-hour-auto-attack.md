# Zero Hour automatic attack

A proposal for one slice. When it ends, each lasting decision moves into the document of the system it governs, and this document is deleted ([A slice's proposal is temporary](../../AGENTS.md#documents)); its plan is in `PLAN.md`. Its facts are in [Zero Hour automatic attack](../research/zero-hour-auto-attack.md).

**At the slice's end** a Zero Hour player's idle units and base defenses attack the enemies that come within their weapons' reach, with no order: a China player's Gattling Cannon fires at an enemy that drives past it, and a tank that stands fires at an enemy that comes in range, and stops once the enemy leaves it.

## Decisions

### Orders

- **An idle unit scans for a target, as its type declares.** A unit type's `acquire = { scan_ms, targets }` makes a unit that stands idle, with no destination, nothing under way and no attack target, take as its attack target, each `scan_ms` and staggered by stable id as `on_think` is, the nearest unit its team sees that one of `targets` selects and that a weapon of it selects and reaches, the lower stable id first at an equal distance. Practice: Zero Hour's `getNextMoodTarget`, as a human player's unit runs it, and OpenRA's `Defend` stance, a human player's default. Refused: a Rhai `on_think` for each unit, which a Zero Hour army of hundreds of idle units would call at each scan, as the test modes' towers still do by their own rules; a range of the scan's own, as the original's vision times 2.2, which its human rule then cuts to the weapons' reach.
- **A scanned target holds only in reach.** An attack target keeps its source, an order or the scan; the chase walks to an ordered one and drops a scanned one that no weapon of the unit reaches any more, so the unit stands idle and scans again at its next turn. Practice: Zero Hour fails a human player's attack of `CMD_FROM_AI` out of its pursuit, and OpenRA's `Defend` attacks without moving. Refused: a chase of a scanned target, which is Zero Hour's rule for a computer player.
- **Every player's units take the human player's rule.** A skirmish AI is a server bot that orders as a player does ([Zero Hour](09-zero-hour.md#decisions), The skirmish AI is a server bot), so a computer player's pursuit within its vision is the bot's to order. Refused: a rule by the kind of player, which the engine does not know.
- **The scan is the server's.** Only a role that computes state scans, as only the server thinks; a predicting client takes the target from the server's state and corrects to it. Refused: a client's own scan, which reads the enemies at their interpolated places, behind the server's, and so mispredicts anyway.
- **A unit that scans neither builds nor gathers.** The load refuses `acquire` on a type with no weapon, and on one with a build or a gather, whose loops stand a unit idle between their steps, as Zero Hour's dozers and workers carry no weapon. Refused: a test of each capability's loop in the scan, which would tie `orders` to `production`.
- **Retaliation, a turret's own scan on the move and stances, planned.** Each with the slice that needs it: an idle ally's attack on a unit that damaged one of its team, a turret that takes its own target while its unit walks, and a player's choice of a unit's stance.

### The import

- **Players regard the neutral team as neutral.** Each player's team regards `neutral`, the team of every object the map places, as `Neutral`, as `PlayerList::newGame` makes every player regard the neutral player, so a scan's `hostiles` passes over a civilian building and a supply dock, and an order's `enemies` still takes them. The import's filters already name the neutral apart, a projectile's `hostiles` for `ENEMIES` and a splash's `neutrals` for `NEUTRALS`, which a hostile neutral team breaks today. Refused: the neutral team as hostile, as 09 now decides it, under which every idle unit would attack the map's civilian objects.
- **A scan follows `AutoAcquireEnemiesWhenIdle`.** A type of an AI module whose field holds `Yes` and of an imported weapon scans each `MoodAttackCheckRate`, 2 s when absent, as the import converts a time ([Zero Hour](09-zero-hour.md#decisions), Thirty frames a second, the original's times); one of `No`, or of no field, does not.
- **Which targets, by the original's filters.** The rules package's `[defaults.acquire]` names the hostile units that are no structure and the hostile base defenses, a new tag of `FS_BASE_DEFENSE`, as `PartitionFilterRejectBuildings` keeps for a human player; a type of `ATTACK_BUILDINGS` writes every hostile unit. Refused: the filters in the importer's code, as the fixed rules are the rules package's data ([Zero Hour](09-zero-hour.md#the-rules-package), The fixed rules are its data).

### Losses

What 09 lists once the slice ends:

- **Retaliation.** An idle unit does not answer an attacker beyond its weapons' reach, where the original sends idle allies after it.
- **A turret on the move.** A unit's turret scans only while its unit stands idle, where the original's turret takes targets while its unit drives.
- **Line of sight.** A scan takes a target its team sees, through what blocks sight, where the original's `ATTACK_NEEDS_LINE_OF_SIGHT` needs a clear line.
- **A melee chase.** A scanned target is dropped out of reach for every weapon, where the original's contact weapons chase it.
- **Stealth.** `Stealthed` is not read, as no unit hides.
