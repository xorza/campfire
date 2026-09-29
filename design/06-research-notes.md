# Campfire — Research Notes

The design holds up against what others have learned. The biggest hidden risk is that an open-source client makes bots and scripts easy to build, which matters most once money is at stake. All six early decisions below are now made.

## Decide early

| # | Decision | Status | What others learned |
| --- | --- | --- | --- |
| 1 | The engine is deterministic from the first line of code | Decided. How is left to the technical design | Making the existing League of Legends server deterministic took [almost a full year of work by several engineers](https://www.riotgames.com/en/news/determinism-league-legends-implementation) |
| 2 | Replays are checked automatically on every OS from day one | Decided | Riot found [only the first divergence matters](https://technology.riotgames.com/news/determinism-league-legends-fixing-divergences), since everything after it drifts apart |
| 3 | Every match can be replayed on the exact engine version it ran on | Decided: every engine release is a tagged version in git, and each match record names its release | League replays [expire with the patch they were recorded on](https://www.riotgames.com/en/news/profiling-real-world-performance-league) |
| 4 | A player's main key stays out of the game and signs a short-lived key per match | Decided | [Nostr remote signing](https://github.com/nostr-protocol/nips/blob/master/46.md) exists to keep private keys out of apps |
| 5 | Hosts pin exact content versions; author updates never reach players automatically | Decided | In the [fractureiser incident](https://prismlauncher.org/news/cf-compromised-alert/), hijacked mod-author accounts, some with two-factor login, pushed malware into popular Minecraft mods |
| 6 | Pick the scripting runtime by its sandbox and update record first | Decided: Rhai | Dota 2 ran an [unsandboxed 2018 build of its JavaScript engine](https://www.gendigital.com/blog/insights/research/dota-2-under-attack-how-a-v8-bug-was-exploited-in-the-game); malicious custom game modes used an old bug in it until Valve patched it in January 2023 |

## Hidden issues

| Issue | Why it matters here | Mitigation |
| --- | --- | --- |
| Bots and scripts are easy with an open-source client | Riot describes a [scripting epidemic](https://www.leagueoflegends.com/en-us/news/dev/dev-vanguard-x-lol/) in League (auto-dodging, auto-attacking) even with a closed client, and moved to kernel-level anti-cheat. An open-source client cannot use client-side anti-cheat at all, and wagers make cheating pay | Accept that the client cannot be trusted. Give hosts server-side options: action-rate and reaction-time limits, behaviour detection plugins, stake caps for new identities, reputation. Favour mechanics that reward decisions over pure reflexes. Let hosts run openly "bots welcome" servers. Decided: server-side protection only, never client-side anti-cheat; the game targets groups that trust each other |
| Empty queues kill new team games | [Paragon](https://mein-mmo.de/en/rip-8-mobas-that-didnt-make-it,214691/) fell into a cycle: fewer players, longer queues, more players leaving | Bots as a first-class feature, small team modes first, singleplayer and LAN, and tools for creators before a big player push |
| Locked Lightning payments are awkward in practice | Many wallets show locked payments poorly or fail on them, [as RoboSats found](https://github.com/robosats/robosats/issues/44); they tie up routing liquidity, and some routing nodes [refuse long holds](https://www.spark.money/glossary/hodl-invoice) | Keep matches short and stakes small; test the common wallets; offer entry-fee-only modes as a fallback |
| Developers of payment software can be prosecuted | A Tornado Cash developer was convicted on one count despite the software being non-custodial, and a retrial on the other counts is [tentatively set, as of April 2026, for October 26, 2026](https://www.bitget.com/news/detail/12560605389267). A US bill to protect non-custodial developers [is pending](https://en.cryptonomist.ch/2026/03/10/roman-storm-trial-retrial/) | Keep payments in a separate, optional package so the engine contains no money code; the project never touches funds or runs infrastructure. (Not legal advice.) |

## Ideas to improve the design

- **Singleplayer and LAN run a local server.** [Veloren](https://book.veloren.net/players/hosting-a-server.html), an open-source Rust RPG, does this: one code path for every mode, and bots exercise the real server.
- **Nostr identity is a real differentiator.** Veloren still needs a [central authentication server](https://github.com/veloren/veloren) so one account works on every server. Nostr keys remove that central piece.
- **Restore crashed matches instead of refunding.** Riot's deterministic server lets esports officials [replay a match to a chosen moment](https://technology.riotgames.com/news/determinism-league-legends-introduction) and players reconnect. The same feature would cut refunds and disputes in wagered matches.
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
