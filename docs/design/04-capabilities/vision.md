# Vision

## Mechanism

- **Vision decides what each client receives.** What each vision group sees is state, and the server replicates each unit only to the clients whose group sees it, so hidden information never reaches a client.
- **Backends.** Grid fog of war now; 3D occlusion and relevance later, the latter as Fortnite's replication graph weighs what is near a client's view.

## Grid fog of war

- **One rule for every unit.** A structure, a projectile and an item on the ground are seen only while in sight; a box stands in every cell it overlaps, so a building half in sight is seen.
- **Brush hides who stands in it,** as League of Legends' brush does: a unit reveals a brush's cells only from inside it, and a ward in a brush sees into it.
- **Reveals detect nothing.** A reveal shows cells, as Farsight and Snow Owl do, and detects no hidden unit.
- **A stage that costs what changed.** Each cell counts each group's sights on it and is seen while its count is above 0, so a seer that stood still costs a compare and a moved one counts only the cells its rows lost and gained, as Zero Hour's `PartitionCell` counts its lookers (`Display.h`, `m_currentShroud`); its price is a tick where every seer crosses a cell, about twice what the bitmaps cost. Refused: the bitmaps cleared and set again each tick, whose cost grows with every seer however still; a sight from the seer's cell, as Zero Hour, Spring's `LosHandler` and the Permafrost engine key theirs, which reveals up to half a cell more or less than the seer's place does; a moved seer's old runs found on the grid again, 67 % more for a tick where every seer moved.
- **A team keeps the cells it explored.** Each vision group keeps the cells it ever saw beside those it sees now, when the mode starts its map shrouded, as Zero Hour's `CELLSHROUD_FOGGED` and OpenRA's explored cells; a MOBA's map starts known. They are state, saved and restored, as no stage can derive them again. Refused: a client's own memory of what it saw, lost on a reconnect and a restore.
- **A neutral building stands in the fog.** On a grid of `neutral_in_fog`, a team sees the units that do not walk of a team it regards as neutral wherever it explored, so a civilian building or a tree of the map stands in the fog from the start, as Zero Hour's `PartitionData::updateShroudStatus` fogs a neutral `KINDOF_IMMOBILE` object and shrouds every other it never saw; a mode's map sets it, as Zero Hour's does ([Zero Hour's shroud and radar](../../research/zero-hour-shroud.md)). Refused: the rule for every mode, where a MOBA's teams set neutral would see each other's towers.
- **Rooms per team.** The server keeps the network stack's rooms, one for each team, and a unit stands in the rooms of the teams that see it ([Replication audiences](../10-replication-audiences.md)).

## Hidden units

- **Detection as Dota 2's true sight and StarCraft II's detectors.** A hidden unit is seen by its own group and by a group whose detecting unit sees a cell it stands in; detection comes from a type's tag or a modifier.

## Planned

- **Senses.** What one unit perceives, sight within a cone and hearing of noises, for AI that sneaks and is sneaked on, as Skyrim's guards notice a thief; the engine gives exact facts, and the mode decides what they mean.
- **Dynamic blockers.** Smoke, doors and built walls block sight while they stand.

## Network

- **A predicting client reveals nothing,** and takes each unit it holds as seen; what each group sees stays the server's.
- **The server sends a team's cells to its player** (decided 2026-10-27). The Vision stage writes, on each player's entity, the cells its team sees and explored, a kind `sight` sent to its owner alone ([Replication audiences](../10-replication-audiences.md)), written only when a cell changes; a predicting client tests a build point by them, and its fog draws them; a busy player of Zero Hour's check receives about 12 changes of 728 bytes a second ([Zero Hour's shroud and radar](../../research/zero-hour-shroud.md)). Refused: the client's own count over its team's units, which shows no reveal, a server resource, and runs one rule in two places; the match's entity, whose cells every client would receive.
