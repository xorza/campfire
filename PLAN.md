# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Zero Hour projectiles

The slice of [Zero Hour projectiles](docs/design/20-zero-hour-projectiles.md): the player trains a tank, and its shells destroy the bot's command center. The systems it changes: `geometry`, `units`, `client-files`, `mode`, `package`, `capabilities`, `projectiles`, `combat`, `import`, `client`, `zero-hour-check`. Each batch ends with `./drive.py check` passing; a golden that moves names its batch.

### 9. The fold

`docs`: the system documents of every system the slice changed. It stands last, as a slice ends when the user plays it.

- [ ] docs/design/20-zero-hour-projectiles.md — the slice's decisions stand in a document of its own (A slice's proposal is temporary). → Once the user plays a Crusader's shells in the client, each lasting decision folds into the document of the system it governs, its practice into `docs/research/precedents.md`, and design 20 is deleted.


## Match options

The slice of [Match options](docs/design/21-match-options.md): the player starts a Zero Hour skirmish with 50,000 money for every player against a bot that builds a base and never attacks. The systems it changes: `package`, `mode`, `protocol`, `runner`, `verifier`, `harness`, `net`, `server`, `client`, `import`, `zero-hour-check`. Each batch ends with `./drive.py check` passing; a golden that moves names its batch.

### 4. Zero Hour's money and the builder bot

`import`: `zero_hour/rules_package.rs`; `zero-hour-check`: its bot's order script, its play and `pins.rs`. It stands last, as it plays every batch before it.

- [ ] crates/apps/import/src/zero_hour/rules_package.rs:66 — `start_money` is a param of one value (The import writes Zero Hour's money as an option). → The option of the values of `Multiplayer.ini`'s choices, with a fixture's test.
- [ ] checks/zero-hour-check/ — the check's bot is idle (A bot that builds and does not fight). → The builder bot's order script; the check plays a skirmish of 50,000 with it as its bot, and pins what it built and trained, and that no unit of it stood farther from its start than its farthest site. Its fingerprints are pinned from a run on Linux and a run on macOS.
