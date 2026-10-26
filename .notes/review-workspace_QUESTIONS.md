# Questions: review-workspace

## Batch 3: the protocol check in the ALPN

**Item.** `net-stack/src/protocol_check_plugin/mod.rs:44,80` sends the registries' hashes as an event of the registries it checks, beside the first data. The decision (11-net-stack/transport.md, The hashes travel in the ALPN, planned) puts them in the UDP link's ALPN.

**Why it needs a call.** The decision was taken on a cost of "small", which was wrong. The hashes exist only once the app's plugins finish (`Plugin::finish` seals the registries, and the server's registries follow the mode's capabilities), but every UDP endpoint is made before that:

- the dedicated server binds and serves its socket before it builds its app (`apps/server/src/main.rs:93`, then `ServerAppSetup::build`);
- the client links and triggers `Connect` before `app.run()` (`apps/client/src/lib.rs:160`), and its app cannot finish early, as the render plugin finishes only once its device is ready;
- the local pair makes both ends at once, before either app exists (`net/src/udp_links.rs`, `UdpLinks::local`).

**Options.**

| Option | What it does | Cost |
| --- | --- | --- |
| A. ALPN, endpoints made after finish | The server builds and finishes its app, then serves its socket with the ALPN; the client links and connects in a `Startup` system; the local pair takes the server app's ALPN on both ends, as one build. `UdpServer::serve`, `UdpClient::endpoint`, `ServerSocket`, `ClientSocket`, `ServerAppSetup`, the local server, the stepper and the harness take the ALPN. | Large: about ten files across net-udp, net-socket, net, net-stepper, net-harness and both apps. |
| B. A reserved channel, sent first (recommended) | The check keeps its event but on a fixed channel and message id outside the registries, sent before anything else, and replication and messages start only once it passed. | Medium: a checked state that the replication and message senders wait on, in net-stack. The check stays in the stack, and every link, plain or sealed, makes it. |
| C. Keep the event, fix only its ids | The check's channel and message take fixed ids outside the registries, so a mismatched build reads `ProtocolMismatch`; data may still flow in the first frame. | Small, but transport.md's "before data flows" stays unmet. |

**Recommendation.** B: it meets "before data flows", it needs no endpoint to move, and it checks the in-process links too.

**Blocked.** Batch 3's last item only. No other item depends on it.

## Batch 15: the factions' params as one table

**Item.** `apps/import/src/zero_hour/rules_package/mod.rs:204-221` writes eight params for each faction, `start_building_<faction>` and the others, and the Zero Hour scripts join their names at run time (`bot.rhai`, `mode.rhai`). The plan's target is `ModeParam::Table` and a `factions.<faction>.{…}` table "that the load checks".

**Why it needs a call.** The load checks only the params a script names by a literal member, `ctx.params.<name>`, which `script_facts` records. A table removes the joined names, but its inner keys, `factions[faction].start_building`, are a Rhai map's, which the load does not see and which give `()` for a misspelt key, where `ctx.params[...]` now fails with the name. So the target groups the data but does not move the failure to the load.

**Options.**

| Option | What it does | Cost |
| --- | --- | --- |
| A. A table, grouping only | `ModeParam::Table` of params by name, read by scripts as a map; the import writes `factions`; the scripts read `ctx.params.factions[faction].<field>`. | Medium: mode data and its reference, the import, both scripts, their tests. A misspelt field gives `()` at run time. |
| B. A table of a declared shape | As A, and a mode declares the table's fields, which `script_facts` checks on each `ctx.params.<table>[…].<field>` chain at the load. | Large: a shape declaration in the mode's data and a chain walk in the script facts. |
| C. Keep the joined names (recommended) | The params stay as they are. | None. A misspelt name still fails at run time, with its name. |

**Recommendation.** C: A trades a failure that names the param for a silent `()`, and B builds a schema for one game's eight fields. Take B when a second game needs tables of params.

**Blocked.** Batch 15's second item only. No other item depends on it.

## Batch 19: "pool" on the script side

**Item.** "Pool" names a script call's operation budget (`scripts/pool/mod.rs`'s `Pool`, `ScriptPools`, `Role::pools()`, `CallError::PoolSpent`, `FailureKind::PoolSpent`), a stat pool (`PoolId`, `PoolCost`, `PoolPurse`, `Pools`), and Bevy's task pools. The plan's target: the script side says "budget".

**Why it needs a call.** The design itself names the script side "pools" (02-engine-core.md, Operation pools: "the player's pool", `think`, `mode`), and the script crate already has a `Budget` struct, which each pool holds, so "budget" needs new names for the enum and the setup, and a decision renamed in the design and in every citation of it.

**Options.**

| Option | What it does | Cost |
| --- | --- | --- |
| A. Rename the script side (recommended) | `Pool` → `BudgetOf` (`Player`, `Think`, `Mode`), `ScriptPools` → `ScriptBudgets`' setup `BudgetSetup`, `Role::pools()` → `Role::budgets()`, `PoolSpent` → `BudgetSpent`, and the decision "Operation pools" → "Operation budgets", with its citations. | Medium: about 20 sites in script, declarations, capabilities, runner, host and tests, the design and the generated reference. |
| B. Keep "pool" on the script side | The design's word stays; stat pools keep `Pool*` types. | None. "Pool" keeps three meanings, against AGENTS.md's "one word, one meaning". |

**Recommendation.** A: the stat pools are a game's data, which modes and scripts name, so the engine's own budget takes the other word.

**Blocked.** Batch 19's second item only. No other item depends on it.
