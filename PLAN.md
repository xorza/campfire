# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

**Slice 5, units that drive.** The user starts Tournament Desert in the client, finds America's command center and dozer at their start, selects the dozer, and drives it round the cliffs to the other start, at its speed and turn rate, on the drawn ground. The design is [Units that drive](docs/design/17-units-that-drive.md); the facts are [Zero Hour research](docs/research/zero-hour.md#movement). The Zero Hour check plays the drive, and loads every imported map.

**Hardening, before the rest of slice 5.** The workspace review, [review-workspace](.notes/review-workspace.md), found about 30 bugs and ten root causes. The user chose to take the whole review before Zero Hour's batch. Each batch removes the mechanism behind its root cause and adds a guard first, which fails on the bugs it fixes. A batch's items, with their places, defects and target shapes, are in the review's batch of the same number; an item done is deleted there, and a batch done is deleted here. The decisions D1 to D5 are in the review's section 0.

## 1. Hardening

### H1. Restore safety

(sim, the core capabilities, `mode`, `items`, `navigation`, `runner`, `server`, `net`; review RC1.) Code that computes on state is total over what its check accepts, in the wide domain of design 09; each kind declares the parts and state markers its entities may carry, and one restore walk refuses any other; a value a book fixes is checked against the book; `SessionRules::check` returns `CheckedTerms`, which every later reader takes. Guard: the drawn-values test fails on a state type with no holder, holds every state type, and plays a scripted window after each restore. First, as the restore bugs are the ones a peer can cause. Goldens: none move.

### H2. Order and roles

(sim, `declare`, every module's install, `combat`, `abilities`, `items`, `vision`, `production`, design 14; review RC8, RC9, D2.) Systems whose output order reaches state walk through `Ordered`; `InstallCx` installs by role through `systems`, `server` and `scripted`, with no `role()`; the server alone pays, refunds and despawns (D2); the capabilities no module builds leave `Capability`; mode draws are keyed by `StableId::MATCH`. Guard: a restore target that spawns in reverse order, checked by the resume test. Goldens: they move, for the RNG key and maybe the order of deaths.

### H3. Package crates

(`package`, `runner`, `verifier`, `mode`, design 02; review D3, D4.) `package` splits into a core crate of bytes and an outer crate that reads directories; a new crate holds the client's formats and `HeightGrid`. Before H4 and H10, so their fixes land in the final crates. Goldens: none move.

### H4. Untrusted files

(`package`, the books, `actions`, `items`, `geometry`; review RC2.) A read takes a `ReadBound`; each binary format checks its sizes in one function; the book builder is the load's check, and `load_check.rs` keeps only the rules that span books. Guard: every truncation and flip of each package binary format is refused. Goldens: none move.

### H5. Store and network bugs

(`store`, `quic`, `net`; review RC10.) One failure and lock-poison helper for the store's workers, a `DurableDir`, a `LinkEnd` case for an app close the handshake hid, and the small QUIC and filter fixes. Goldens: none move.

### H6. Client and harness

(`client`, `harness`, `lan-check`; review RC8.) The client and the harness ask `PlayerUnits` who a player commands; the HUD rebuilds its rows when the slots change. Before Zero Hour's batch, whose units the client otherwise draws as avatars. Goldens: none move.

### H7. The module graph

(`common`'s tests, `units`, `declare`, `players`, `script`, `net`, `orders`; review RC7.) Guard first: the cycle test checks the siblings under every parent. Then the wrapper modules go and each cycle is cut at its edge against the layering. Goldens: none move.

### H8. Declarations own install and spawn

(`declare`, the release's installer, `units`, `combat`, `navigation`, `stats`, `mode`, `orders`, `production`, `deliveries`, `progression`, the books; review RC5.) After H2 and H7. `InstallCx` keeps its world private, so resources, hooks and parts come from declarations; every part comes with its spawn; navigation owns the map walk check; stats owns passives; progression asks stats for a level change. Guard: a unit changes archetype after its spawn only by its kind's state markers. Goldens: they may move where a part now comes at the spawn.

### H9. One home for each session fact

(`protocol`, `runner`, `package`, `server`, `net`, `math`, `store`; review RC3.) After H1. `InputRules` nests in the terms, one lobby opening serves both servers, one snapshot check, one offering, data filters from the state-type walk, a protocol `Signer`, and the shared types in `common`. Goldens: none move.

### H10. One home for each format

(`import`, the client formats' crate, `mode`, `client`, `math`; review RC3.) After H3. The import writes through the reader's types, one KTX2 codec, one name table of templates, one client-data owner. Guard: a round-trip test for each format the import writes. Goldens: none move.

### H11. Change-driven passes

(sim, `navigation`, `stats`, `actions`, `orders`, `production`, `projectiles`, `abilities`, `runner`, `protocol`, `quic`, `geometry`; review RC4.) A `DueRun` in sim for timed work, `Changed` filters for the rest, and incremental static and region updates. Each item names its bench for the next optimization session. Goldens: none move.

### H12. The published surface

(the world traits, `net`, `codec`, `schedule`, `store`, `log`, `package`, `runner`; review RC6.) After H7. Guard first: the world-trait test fails on a `*WorldCrate` import outside its crate. Then `internals` features for the base crates, private fields, and the narrowest visibility. Goldens: none move.

### H13. Names resolved at the load

(`stats`, `mode`, `script`, `actions`, `values`, `protocol`, `net`, `quic`.) Applier params, marker tags, `PackageId`, the main key, slot plans and refusals become typed at their boundary. Goldens: none move.

### H14. Sibling APIs

(`values`, `geometry`, `actions`, `deliveries`, `players`, `units`, `script`, `stats`, `combat`, `abilities`, `net`, `quic`, the apps, `harness`; review RC10.) One shape for ids, sets, placed bodies and callback tables; flags become enums; wide signatures become parameter structs. Goldens: none move.

### H15. Design and code that disagree

(design 02 and 12, the release's tests, `common`'s tests, the checks; review D5.) Guard: every crate has a layer, and the test checks "one above the highest dependency". Design 12 D7 stays, and the fingerprint comparison and the parity suite join this plan as open steps. Goldens: none move.

## 2. Zero Hour's units drive

`import`: `zero_hour`; `source/packages/zero-hour`; `checks/zero-hour-check`. It needs the turn rate, the load's rules and the selection to play, and goes after the hardening.

### U6. Unit types move and have bodies

(`import`; Units that drive D1, D4, Data.) The import reads `Locomotor.ini` and each object's `Locomotor = SET_NORMAL`, its `KindOf` and its `Geometry`; each unit type gains `move_speed` and `turn_rate` from a locomotor of `GROUND`, and its body by D4's rule. The mode's manifest gains `stats`, `orders` and `navigation`, and `max_move_speed` the largest speed it writes. Check: fixture objects of a ground locomotor, an air one, a box structure, a small structure, a cylinder structure and no geometry give the unit types computed by hand.

### U7. Cliffs are walls

(`import`; Units that drive D3.) Each map's `[navigation]` gains `cell = 10` and its walls: its cliff cells, the bits of version 7 and 8 or the heights' rule of version 6, grown by one ring and merged into rectangles row by row, on the engine's axes. Check: a fixture map's cliffs, grown and merged, give the walls computed by hand; the Zero Hour check loads every imported map in the imported mode, and pins Tournament Desert's 286 walls.

### U8. The skirmish's start

(`import`, the rules package; Units that drive D6.) The import reads `PlayerTemplate.ini`, tags each `Player_<n>_Start` and `Player_<n>_Rally` waypoint `start`, and gives the mode `[navigation] spawn_tags = ["start"]` and the `[params]` of America's start; the rules package's mode script spawns, for the player at start position `n`, the building at `Player_<n>_Start` and each starting unit at `Player_<n>_Rally`, or at the building's point moved `start_offset` along `+z`, owned by the player. Check: the import's fixture install's params and tags; the Zero Hour check finds the command center at `Player_1_Start` and the dozer clear of its box, owned by the player; the dozer, ordered to `Player_2_Start`, arrives within a bound of ticks the check pins and logs, and never stands in a wall's cell; the user drives it in the client.

## Foundations


