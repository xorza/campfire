# LAN check

Design: [Modules](../design/02-engine-core.md#modules), `lan-check`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

- The check builds the server, the client and the verifier in one cargo invocation, so cargo unifies their dependencies' features: the server and the verifier it runs carry the client's Bevy features, such as `bevy_ecs`'s `multi_threaded`, which runs a schedule's systems in parallel, and `bevy_reflect`'s `auto_register`, which neither has when its package is built alone. The binaries the check plays and verifies differ from the ones each package builds. Which binaries must it check? (a) The unified ones, as now: one build of the workspace. (b) Each package's own: a build for each binary, up to four builds of the workspace. (c) The unified ones, made each package's own: the server and the verifier ask for the features the client brings, so each package alone builds what the check runs: one build, but the server and the verifier change. (d) The unified ones until cargo's workspace feature unification, still unstable in cargo 1.99, is stable, then one feature set for every build.

## Research

- **Plan: F10.** The check starts bot 1 again as soon as the server logs bot 1's first order, while the other bot's order of the same stamp may still be on its way, and it fails an order that misses its stamp tick. In CI run 37523961750 on Ubuntu, attempt 1, the server logged bot 1's order stamped 20 at 20:13:55.960; bot 0 sent its own at 55.971, and the server logged it at 56.086, 115 ms later, in tick 21; the new bot 1 process started in that window and joined at 56.108. On a runner of 4 cores, the new process's start shares them with the server and bot 0; the logs hold no time of each frame or packet, so they do not show which process waited.

## Ready

