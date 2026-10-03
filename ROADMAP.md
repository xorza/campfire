# Roadmap

The stages to 1.0, in order, open items only: remove an item when it is done, and a stage when it is empty. Each stage ends when its test passes. The next concrete steps of the current stage are in [PLAN.md](PLAN.md); a later stage's steps go there when it starts, after its design step. Milestones: [Campfire](docs/design/01-campfire-design.md#milestones).

## Milestone 1 — Playable on LAN

The 3v3 MOBA with bots, on LAN or a local server, its logs verified and a crashed match restored; the game model proven on a tiny mode of every target genre. Players use local Nostr key files; no relays, listings, launcher or payments.

### 3. Vertical slice, close

- The collision bench on a quiet machine, its numbers beside the collision design.

Done when the bench shows that collision fits the Collide stage's share of a tick at 30 Hz.

### 5. MOBA mechanics

- Progression: points, the `learn` order and perks ([Progression](docs/design/04-capabilities/progression.md)).
- Action values and bookkeeping; channels, toggles and `ctx.reveal` ([Actions](docs/design/04-capabilities/actions.md)).
- Forced movement: dash, knock back, teleport ([Navigation](docs/design/04-capabilities/navigation.md#forced-movement)).
- The core's planned script names: `ctx.pick` and `ctx.chance` from the secret stream, and the vector operators `*`, `+` and `-` ([Script API reference](docs/design/08-script-api-reference.md)).
- Terrain in the map: the cells each layer's map blocks, which routes go round and the pathing grid holds, for walls and the jungle ([Navigation](docs/design/04-capabilities/navigation.md#data)); brush that blocks sight from outside it ([Vision](docs/design/04-capabilities/vision.md#grid-fog-of-war)).
- The package load refuses a planned name, once no reference package uses one: design 08 accepts a planned name at load, so a script that uses one loads and fails at each call, as Rime's Snow Owl does on `ctx.reveal`.
- Items for the MOBA: inventory, equipment, the shop, item actions and passives ([Items](docs/design/04-capabilities/items.md)).
- The 3v3 scenario's players learn and cast their heroes' abilities, beside the attacks, spells, towers and camps of stage 4's scenario.

Done when every mechanic design 07 lists runs from the reference heroes' and spells' packages, and none uses a planned name.

### 6. Sessions

- Checkpoints, crash restore within the restore window, reconnect with the same session key, late join, receipts.
- Inputs after a server stall: a frame that catches up runs all its ticks before it reads the inputs that arrived in the meantime, so on-time inputs take effect up to a whole burst late. Decide whether the server reads inputs between those ticks, or bounds the burst ([Session log](docs/design/05-protocol-spec.md#session-log)).
- Players joining and leaving, with their hooks, as the log records them.
- The local server on a client thread, pause and game speed; saves and loads on one release ([Singleplayer and saves](docs/design/01-campfire-design.md#singleplayer-and-saves)).

Done when a LAN match whose server is killed restores and ends, its log verifying; a client that leaves comes back; and a match against bots runs on a local server with no network.

### 7. Genre proofs: RTS and campaign

- Summons: the `spawn` effect, unit types in packages other than the mode, and a timed life ([Effects](docs/design/04-capabilities/actions.md#effects)).
- First cuts: `production` (build, gather; the train's cancel, refund, rally points, requirements and supply), region events, `quests` objectives, carry and campaigns, save converters.
- The RTS skirmish and the RTS mission ([Genre proofs](docs/design/04-capabilities/genres.md#genre-proofs)).

Done when both play to their goldens on every OS in CI.

### 8. Genre proofs: shooter and battle royale

- First cuts: `character` and level geometry, `hitboxes` (rays with lag compensation), `interaction` (the `use` action and its `blocks = ["use"]` tag effect, for planting and defusing the bomb), item units on the ground, random tables, relevance, teams beyond 63.
- The CS round and the BR zone.

Done when both play to their goldens on every OS in CI.

### 9. Genre proofs: RPG and MMO

- First cuts: `quests` and dialogue, `world` (dormant regions), the game clock, senses, crafting, tracks and perks, sweeps, package overrides, generated maps, scripted systems.
- The MMO zone, the Diablo level and the RPG town; the test that every pair of capabilities meets in a test mode ([Testing combinations](docs/design/04-capabilities/00-overview.md#testing-combinations)).

Done when all three play to their goldens on every OS in CI, and the pair test passes.

### 10. Reference game

- The 3v3 two-lane map, six heroes, about 20 items, structures that fall in order, the neutral objective, whole camps and streak bounties, team-view bots ([Reference MOBA](docs/design/07-reference-moba.md)).
- The client: interpolation, presentation scripts and events, assets within their limits; areas drawn where they lie and as far as they reach; the mode's and the units' script state sent to each client as each field's `sync` says ([Script state](docs/design/03-game-scripting.md#script-state)).
- Creator tools: data schemas and hot reload ([Creator tools](docs/design/02-engine-core.md#creator-tools)).

Done when two humans and four bots play a whole 3v3 on LAN to its end, and its log verifies on every OS.

### 11. Hardening

- Bot matches of the reference MOBA in CI on every commit, each OS's session logs replayed on every other; each capability's worst tick measured against its stated cost; log tamper tests.
- mDNS; flood limits before signature checks; NIP-49 encrypted key files; several identities in the client.
- Anti-cheat: detection plugins that read session logs.

Done when a week of CI runs finds no divergence and every worst tick fits its budget.

## Milestone 2 — Open network

### 12. Nostr

- The campfire kinds and their field lists; server listings, package announcements, packages over Blossom, published session logs, a world's after its delay ([Nostr events](docs/design/05-protocol-spec.md#nostr-events)).
- Reputation labels, review requests and community verdicts, ban lists, server groups.

Done when a client finds a server through a relay, fetches its packages from Blossom, plays, and reads the published log.

### 13. Releases and launcher

- Engine release events signed by k of n keys, revocation and key-set changes; reproducible builds; the launcher and its server browser.
- The NIP proposal of campfire's kinds.

Done when a launcher refuses a release with too few signatures and starts one with enough.

## Milestone 3 — Payments

The optional `payments` module, in its own repository ([Lightning payments](docs/design/01-campfire-design.md#lightning-payments)).

### 14. Fees and pools

- Entry fees; wager pools with hold invoices and arbiters; rewards and failed payouts.

Done when a wagered LAN match pays its winners, and an aborted one returns every stake.

### 15. Charges, sales and ownership

- Time-based and per-event charges within a wallet's budget; item sales by atomic swap ([Item sale](docs/design/05-protocol-spec.md#item-sale)); spectator bets.
- Character and skin licenses, and the marketplace.

Done when an item sells between two players with no one holding the money, and a licensed skin is enforced on a server that asks for it.

## Milestone 4 — Full capabilities

### 16. Shooters

- `character` and `hitboxes` at depth, 64 to 128 Hz, 3D occlusion and smoke; projectiles that fall under `gravity`, for grenades and bullet drop ([Hitboxes](docs/design/04-capabilities/hitboxes.md)).

### 17. RTS

- Flow fields, ORCA crowds and formations; production at depth; battles of 1,000 units within the tick budget and the bandwidth.

### 18. Large worlds

- `world` at depth: dormancy, deterministic multithreading, streaming; the relevance backend at 1,000 players; item export between worlds ([Item export](docs/design/05-protocol-spec.md#item-export)).

### 19. Vehicles

- `physics` with a deterministic backend, heightmap terrain, its goldens on every OS.

### 20. Editors and content

- Map and content editors; the reference MOBA's 5v5 on three lanes.
