# LAN check

Design: [Modules](../design/02-engine-core.md#modules), `lan-check`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- The check builds the server, the client and the verifier in one cargo invocation, so cargo unifies their dependencies' features: the server and the verifier it runs carry the client's Bevy features, such as `bevy_ecs`'s `multi_threaded` and `bevy_reflect`'s `auto_register`, which neither has when its package is built alone. The binaries the check plays and verifies differ from the ones each package builds.
- CI's LAN check failed on Ubuntu in run 37523961750: slot 1's order stamped 20 reached the server 115 ms after the bot sent it, about 2.4 ticks ahead of the server, as the check started bot 1 again, and it took effect in tick 21; the check fails an order that misses its stamp tick. The run before passed on every platform with the same runtime code.
- CI's LAN check failed on macOS in run 37537426577: bot 1, started again by the check, logged a warning that it refused a receipt from the server because "the client plays no match", and the check fails a warning it does not expect. Ubuntu and Windows passed with the same runtime code, as did every platform in run 37532577671.

## Ready
