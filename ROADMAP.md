# Roadmap

The stages to 1.0, in order, open items only: remove an item when it is done, and a stage when it is empty. Each stage ends when its test passes. The next concrete steps of the current stage are in [PLAN.md](PLAN.md); a later stage's steps go there when it starts, after its design step. Milestones: [Campfire](docs/design/01-campfire-design.md#milestones).

## Milestone 1 — Playable on LAN

The 3v3 MOBA with bots, on LAN or a local server, its logs verified and a crashed match restored; the game model proven on a tiny mode of every target genre. Players use local Nostr key files; no relays, listings, launcher or payments.

### 7. Genre proofs: RTS and campaign

- Summons: the `spawn` effect of an avatar's or a loadout's action, and unit types in packages other than the mode ([Effects](docs/design/04-capabilities/actions.md#effects)).
- First cuts: `production` (build, gather; the train's cancel, refund, rally points, requirements and supply), region events, `quests` objectives, carry and campaigns, save converters.
- The RTS skirmish and the RTS mission ([Genre proofs](docs/design/04-capabilities/genres.md#genre-proofs)).

Done when both play to their goldens on every OS in CI.

### 8. Genre proofs: shooter and battle royale

- First cuts: `character` and level geometry, `hitboxes` (rays with lag compensation), `interaction` (the `use` action and its `blocks = ["use"]` tag effect, for planting and defusing the bomb), item units on the ground, random tables, relevance, with the components that go only to a unit's owner or team, teams beyond 63.
- The CS round and the BR zone.

Done when both play to their goldens on every OS in CI.

### 9. Genre proofs: RPG and MMO

- First cuts: `quests` and dialogue, `world` (dormant regions), the game clock, senses, crafting, perks ([Progression](docs/design/04-capabilities/progression.md#data)), sweeps, package overrides, generated maps, scripted systems.
- The MMO zone, the Diablo level and the RPG town; the test that every pair of capabilities meets in a test mode ([Testing combinations](docs/design/04-capabilities/00-overview.md#testing-combinations)).

Done when all three play to their goldens on every OS in CI, and the pair test passes.

### 10. Reference game

- The 3v3 two-lane map, six heroes, about 20 items, structures that fall in order, the neutral objective, whole camps and streak bounties, team-view bots ([Reference MOBA](docs/design/07-reference-moba.md)).
- The client: interpolation, presentation scripts and events, assets within their limits; areas drawn where they lie and as far as they reach; the mode's and the units' script state sent to each client as each field's `sync` says ([Script state](docs/design/03-game-scripting.md#script-state)).
- Multiplayer pause and game speed, by the host's setting of who may pause: each client pauses and changes its tick length with the server ([Sessions](docs/design/10-sessions.md#decisions), D6).
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
