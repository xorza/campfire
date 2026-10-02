# Campfire — Research Notes

The design holds up against what others have learned. The biggest hidden risk is that an open-source client makes bots and scripts easy to build, which matters most once money is at stake. All seven early decisions below are now made.

## Decide early

| # | Decision | Status | What others learned |
| --- | --- | --- | --- |
| 1 | The engine is deterministic from the first line of code | Decided. How is left to the technical design | Making the existing League of Legends server deterministic took [almost a full year of work by several engineers](https://www.riotgames.com/en/news/determinism-league-legends-implementation) |
| 2 | Replays are checked automatically on every OS from day one | Decided | Riot found [only the first divergence matters](https://technology.riotgames.com/news/determinism-league-legends-fixing-divergences), since everything after it drifts apart |
| 3 | Every match can be replayed on the exact engine version it ran on | Decided: every engine release is a tagged version in git, and each match record names its release | League replays [expire with the patch they were recorded on](https://www.riotgames.com/en/news/profiling-real-world-performance-league) |
| 4 | A player's main key stays out of the game and signs a short-lived key per session | Decided | [Nostr remote signing](https://github.com/nostr-protocol/nips/blob/master/46.md) exists to keep private keys out of apps |
| 5 | Hosts pin exact content versions; author updates never reach players automatically | Decided | In the [fractureiser incident](https://prismlauncher.org/news/cf-compromised-alert/), hijacked mod-author accounts, some with two-factor login, pushed malware into popular Minecraft mods |
| 6 | Pick the scripting runtime by its sandbox and update record first | Decided: Rhai | Dota 2 ran an [unsandboxed 2018 build of its JavaScript engine](https://www.gendigital.com/blog/insights/research/dota-2-under-attack-how-a-v8-bug-was-exploited-in-the-game); malicious custom game modes used an old bug in it until Valve patched it in January 2023 |
| 7 | One model of units, tags, stats, pools, modifiers, actions, effects, relations and space serves every genre; the core has no genre words ([The model](04-capabilities/00-overview.md#the-model)) | Decided, after the reference MOBA had put its rules into the core: fixed states and League of Legends stats, at most 63 teams, one attack and one resource a unit, path ends for two teams, every distance on the ground plane, and a separate mechanism for each kind of action | Engines that serve many genres converge on it: Unreal's [Gameplay Ability System](https://ikrima.dev/ue4guide/gameplay-programming/gameplay-ability-system/epic-technical-brief/) (shooters, MOBAs, RPGs) has one tag set and one modifier formula; the [StarCraft II data editor](https://wiki.hiveworkshop.com/index.php/Data_Editor), heir of the editor DotA was built in, makes missiles units and runs weapons and abilities through the same effects; Unreal gives [teams attitudes](https://dev.epicgames.com/documentation/unreal-engine/API/Runtime/AIModule/FGenericTeamId); Fortnite sends 100 players what a [spatial grid](https://www.unrealengine.com/en-US/tech-blog/replication-graph-overview-and-proper-replication-methods) finds near each; Photon Quantum runs [MOBAs, RTS games and shooters](https://doc.photonengine.com/quantum/current/getting-started/which-sdk) on one deterministic fixed-point ECS |

## Hidden issues

| Issue | Why it matters here | Mitigation |
| --- | --- | --- |
| Bots and scripts are easy with an open-source client | Riot describes a [scripting epidemic](https://www.leagueoflegends.com/en-us/news/dev/dev-vanguard-x-lol/) in League (auto-dodging, auto-attacking) even with a closed client, and moved to kernel-level anti-cheat. An open-source client cannot use client-side anti-cheat at all, and wagers make cheating pay | Accept that the client cannot be trusted. Give hosts server-side options: action-rate and reaction-time limits, behaviour detection plugins, stake caps for new identities, reputation. Favour mechanics that reward decisions over pure reflexes. Let hosts run openly "bots welcome" servers. Decided: server-side protection only, never client-side anti-cheat; the game targets groups that trust each other first, and strangers play at their own risk |
| Empty queues kill new team games | [Paragon](https://mein-mmo.de/en/rip-8-mobas-that-didnt-make-it,214691/) fell into a cycle: fewer players, longer queues, more players leaving | Bots as a first-class feature, small team modes first, singleplayer and LAN, and tools for creators before a big player push |
| Locked Lightning payments are awkward in practice | Many wallets show locked payments poorly or fail on them, [as RoboSats found](https://github.com/robosats/robosats/issues/44); they tie up routing liquidity, and some routing nodes [refuse long holds](https://www.spark.money/glossary/hodl-invoice) | Keep matches short and stakes small; test the common wallets; offer entry-fee-only modes as a fallback |
| Items sold for money change a game | Blizzard closed Diablo III's [real-money auction house](https://mein-mmo.de/en/morhaime-explains-the-birth-defect-of-diablo-3-and-how-blizzard-got-rid-of-it,364965/) in 2014: buying gear had replaced finding it, and players believed drops were cut to push the market. Belgium counts [random rewards with resale value](https://blog.promise.legal/loot-box-laws-game-developers/) as gambling | Tradable is a choice of each item type, off by default; the host turns sales on; the design names the gambling risk of random loot that sells. Ownership stays with the world that runs the item, not a token: [Taproot Assets](https://docs.lightning.engineering/the-lightning-network/taproot-assets/taproot-assets-on-lightning) and RGB can carry collectibles over Lightning, but only the world decides what an item does |
| Developers of payment software can be prosecuted | A Tornado Cash developer was convicted on one count despite the software being non-custodial, and a retrial on the other counts is [tentatively set, as of April 2026, for October 26, 2026](https://www.bitget.com/news/detail/12560605389267). A US bill to protect non-custodial developers [is pending](https://en.cryptonomist.ch/2026/03/10/roman-storm-trial-retrial/) | Keep payments in a separate, optional package so the engine contains no money code; the project never touches funds or runs infrastructure. (Not legal advice.) |

## Ideas to improve the design

- **Singleplayer and LAN run a local server.** Adopted: the server runs as a thread of the client ([Singleplayer and saves](01-campfire-design.md#singleplayer-and-saves)). [Veloren](https://book.veloren.net/players/hosting-a-server.html), an open-source Rust RPG, does this, and Minecraft [merged singleplayer into multiplayer](https://vip-develop.destructoid.com/?p=107954) in version 1.3: one code path for every mode, and bots exercise the real server.
- **Saves load on newer releases.** Adopted: each release converts the save format before it, one way, as [Factorio](https://direct.factorio.com/blog/post/fff-270) and Minecraft's [DataFixerUpper](https://minecraft.net/article/programmers-play-minecrafts-inner-workings) do, so a campaign or a character outlives an update.
- **Nostr identity is a real differentiator.** Veloren still needs a [central authentication server](https://github.com/veloren/veloren) so one account works on every server. Nostr keys remove that central piece.
- **Restore crashed matches instead of refunding.** Adopted: a restart replays the match's own log. Riot's deterministic server lets esports officials [replay a match to a chosen moment](https://technology.riotgames.com/news/determinism-league-legends-introduction) and players reconnect. The same feature would cut refunds and disputes in wagered matches.
- **Court creators before players.** DotA itself and Dota Auto Chess started as community mods; Auto Chess became [Valve's Dota Underlords](https://en.wikipedia.org/wiki/Dota_Underlords). Shipping the editor, bots and LAN early lets creators arrive before there is an audience.
- **Learn from earlier Lightning games.** Lightnite charged and paid sats per hit, and Bitcoin Bounty Hunt was [discontinued in 2021](https://www.spark.money/tools/bitcoin-gaming-platform-comparison); both tried per-event payments before this project. ZBD and THNDR show Lightning payouts work at scale for simpler games.

## Sources

- [Determinism in League of Legends: Introduction](https://technology.riotgames.com/news/determinism-league-legends-introduction), [Implementation](https://www.riotgames.com/en/news/determinism-league-legends-implementation) and [Fixing Divergences](https://technology.riotgames.com/news/determinism-league-legends-fixing-divergences) — Riot Games
- [Profiling: Real World Performance in League](https://www.riotgames.com/en/news/profiling-real-world-performance-league) — Riot Games
- [/dev: Vanguard x LoL](https://www.leagueoflegends.com/en-us/news/dev/dev-vanguard-x-lol/) — Riot Games
- [Rapier determinism guide](https://rapier.rs/docs/user_guides/rust/determinism/) and [rapier3d determinism test](https://docs.rs/crate/rapier3d/latest/source/tests/simd_backend_determinism.rs)
- [Don't use Lockstep in RTS games](https://medium.com/@treeform/dont-use-lockstep-in-rts-games-b40f3dd6fddb) — why peer-to-peer lockstep allows map hacks
- [Dota 2 under attack: how a V8 bug was exploited](https://www.gendigital.com/blog/insights/research/dota-2-under-attack-how-a-v8-bug-was-exploited-in-the-game) — Avast / Gen Digital
- [fractureiser malware warning](https://prismlauncher.org/news/cf-compromised-alert/) — Prism Launcher
- [NIP-46: Nostr Remote Signing](https://github.com/nostr-protocol/nips/blob/master/46.md)
- [Nostr Wallet Connect (NIP-47) budgets](https://github.com/getAlby/nostr-wallet-connect) — Alby
- [Hodl invoice risks](https://www.spark.money/glossary/hodl-invoice) — Spark glossary; [RoboSats wallet compatibility notes](https://github.com/robosats/robosats/issues/44)
- [Bitcoin gaming platforms compared](https://www.spark.money/tools/bitcoin-gaming-platform-comparison) — Spark
- [Tornado Cash developer retrial date](https://www.bitget.com/news/detail/12560605389267) (Cryptopolitan via Bitget, April 2026) and [retrial request and pending bill](https://en.cryptonomist.ch/2026/03/10/roman-storm-trial-retrial/) (The Cryptonomist)
- [8 MOBAs that didn't make it](https://mein-mmo.de/en/rip-8-mobas-that-didnt-make-it,214691/) — Mein-MMO
- [Veloren: hosting a server](https://book.veloren.net/players/hosting-a-server.html) and [Veloren repository](https://github.com/veloren/veloren)
- [Dota Underlords](https://en.wikipedia.org/wiki/Dota_Underlords) — Wikipedia
- [Determinism](https://box2d.org/posts/2024/08/determinism/) — Box2D v3: cross-platform float determinism without FMA and with own trig
- [Fixed point](https://doc.photonengine.com/quantum/v3/manual/quantum-ecs/fixed-point) — Photon Quantum: Q48.16 fixed-point in place of floats
- [RFC 3514: float semantics](https://rust-lang.github.io/rfcs/3514-float-semantics.html) — Rust: IEEE 754 except NaN bit patterns
- [Philox](https://numpy.org/devdocs/reference/random/bit_generators/philox.html) — NumPy: counter-based, no claim of cryptographic security
- [Accountable Virtual Machines](https://people.mpi-sws.org/~druschel/publications/avm.pdf) — Haeberlen et al., OSDI 2010: tamper-evident logs and authenticators, tested on Counter-Strike
- [NIP-26](https://github.com/nostr-protocol/nips/blob/master/26.md) (unrecommended) and [NWC hold invoice extension](https://github.com/nostr-wallet-connect/nwc/blob/main/03.md)
- [TUF security](https://theupdateframework.io/docs/security/) — The Update Framework: compromise resilience, thresholds, revocation
- [RFC 5929](https://www.rfc-editor.org/rfc/rfc5929) — TLS channel bindings, including `tls-server-end-point`
