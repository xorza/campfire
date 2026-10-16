# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Zero Hour projectiles

The slice of [Zero Hour projectiles](docs/design/20-zero-hour-projectiles.md): the player trains a tank, and its shells destroy the bot's command center. The systems it changes: `geometry`, `units`, `client-files`, `mode`, `package`, `capabilities`, `projectiles`, `combat`, `import`, `client`, `zero-hour-check`. Each batch ends with `./drive.py check` passing; a golden that moves names its batch.

### 9. The fold

`docs`: the system documents of every system the slice changed. It stands last, as a slice ends when the user plays it.

- [ ] docs/design/20-zero-hour-projectiles.md — the slice's decisions stand in a document of its own (A slice's proposal is temporary). → Once the user plays a Crusader's shells in the client, each lasting decision folds into the document of the system it governs, its practice into `docs/research/precedents.md`, and design 20 is deleted.


## Match options

The slice of [Match options](docs/design/21-match-options.md): the player starts a Zero Hour skirmish with 50,000 money for every player against a bot that builds a base and never attacks. The systems it changes: `package`, `mode`, `protocol`, `runner`, `verifier`, `harness`, `net`, `server`, `client`, `import`, `zero-hour-check`. Each batch ends with `./drive.py check` passing; a golden that moves names its batch.

### 5. The fold

`docs`: the documents of the systems the slice changed. It stands last, as a slice ends when the user plays it.

- [ ] docs/design/21-match-options.md — the slice's decisions stand in a document of its own (A slice's proposal is temporary). → Once the user plays a skirmish of 50,000 against the builder bot, each lasting decision folds into the document of the system it governs, its practice into `docs/research/precedents.md`, the protocol's link to it moves to its new home, and design 21 is deleted.

