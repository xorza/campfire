# Roadmap

What 1.0 needs, by milestone and stage, open items only: remove an item when it is done, and a stage when it is empty. Work goes in playable slices ([Workflow](AGENTS.md#workflow)): a slice takes the items it needs from any stage, and the steps of the current slice are in [PLAN.md](PLAN.md). A stage ends when its test passes. Milestones: [Campfire](docs/design/01-campfire-design.md#milestones).

## Milestone 1 — Zero Hour on LAN

Generals: Zero Hour, imported from the player's own copy, plays a skirmish with humans and its AI on LAN or a local server, its logs verified and a crashed match restored, within the parity measure of the original ([Zero Hour](docs/design/12-zero-hour.md)). Players use local Nostr key files; no relays, listings, launcher or payments.

### 7. Import and oracle

- The `import` app and its `zero_hour` module: an install's archives, INI, maps, models, textures, audio and strings into one package, the same bytes on every OS; the game versions it knows.
- The oracle: a GeneralsX fork that plays a replay headless and writes each frame's objects and events; replays imported as Campfire inputs; the parity test and its measure.

Done when a retail replay plays to the same end in the oracle on Linux and macOS, and an import gives the same fingerprint on both.

### 8. Zero Hour skirmish

- The rules package: the module audit, each module type to a capability, a script or a backend; navigation's movement interface, and Zero Hour's pathfinder and locomotors as its pathfinding and movement backends; general's powers and sciences, upgrades, veterancy, containers for garrisons and transports, stealth and detection, crates, superweapons.
- Production as Zero Hour uses it: dozers that build, supply docks and gatherers, money over time; what each client receives of production, and the components that go only to a unit's owner or team.
- The client: interpolation; presentation systems that read data, a unit's model and animation by its condition state, effect lists, particles, terrain, the control bar; areas drawn where they lie; assets within their limits; the mode's and the units' script state sent to each client as each field's `sync` says ([Script state](docs/design/03-game-scripting.md#script-state)).
- The skirmish AI, as server bots ([Sessions](docs/design/10-sessions.md#decisions), D4), and the parity test's bot inputs from the oracle's AI commands ([Zero Hour](docs/design/12-zero-hour.md#decisions), D9).
- Multiplayer pause and game speed, by the host's setting of who may pause: each client pauses and changes its tick length with the server ([Sessions](docs/design/10-sessions.md#decisions), D6).

Done when two humans and the AI play a Zero Hour skirmish on LAN to its end, its log verifies on Linux and macOS, and the parity suite's replays play within the measure.

### 9. Hardening

- AI matches of Zero Hour on every commit, each OS's session logs replayed on every other; each capability's worst tick measured against its stated cost; log tamper tests.
- mDNS; flood limits before signature checks; NIP-49 encrypted key files; several identities in the client.
- Anti-cheat: detection plugins that read session logs.

Done when a week of runs finds no divergence and every worst tick fits its budget.

## Milestone 2 — Open network

### 10. Nostr

- The campfire kinds and their field lists; server listings, package announcements, packages over Blossom, the import recipes listings name ([Packages](docs/design/05-protocol-spec.md#packages)), published session logs ([Nostr events](docs/design/05-protocol-spec.md#nostr-events)).
- Reputation labels, review requests and community verdicts, ban lists, server groups.

Done when a client finds a server through a relay, fetches its packages from Blossom, plays, and reads the published log.

### 11. Releases and launcher

- Engine release events signed by k of n keys, revocation and key-set changes; reproducible builds; the launcher and its server browser.
- Save converters, which carry a save to the next release ([Singleplayer and saves](docs/design/01-campfire-design.md#singleplayer-and-saves)).
- The NIP proposal of campfire's kinds.

Done when a launcher refuses a release with too few signatures and starts one with enough.

## Milestone 3 — Payments

The optional `payments` module, in its own repository ([Lightning payments](docs/design/01-campfire-design.md#lightning-payments)).

### 12. Fees and pools

- Entry fees; wager pools with hold invoices and arbiters; rewards and failed payouts.

Done when a wagered LAN match pays its winners, and an aborted one returns every stake.

### 13. Charges and ownership

- Time-based and per-event charges within a wallet's budget; spectator bets.
- Character and skin licenses, and the marketplace.

Done when a charge past a wallet's budget is refused, and a licensed skin is enforced on a server that asks for it.

## Milestone 4 — More imports

### 14. Zero Hour campaign

- Map scripts run through the rules package's library of condition and action types; region events, objectives, cutscenes, the campaign's missions in order, the challenge mode; saves.

Done when a campaign mission plays to its end, and its replay plays within the measure.

### 15. Worms 4: Mayhem

- Research of its formats; its `import` module and rules package.
- `physics` with a deterministic backend, destructible terrain, the `spatial` metric, projectiles that fall under `gravity` and drift with wind, turns.

Done when a Worms 4 match plays to its end on LAN, and its log verifies on Linux and macOS.

### 16. Creator tools

- Data schemas and hot reload ([Creator tools](docs/design/02-engine-core.md#creator-tools)); map and content editors.
