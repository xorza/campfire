# LAN check

Design: [Modules](../design/02-engine-core.md#modules), `lan-check`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- The check starts bot 1 again as soon as the server logs bot 1's first order, while the other bot's order of the same stamp may still be on its way, and it fails an order that misses its stamp tick. In CI run 37523961750 on Ubuntu, attempt 1, the server logged bot 1's order stamped 20 at 20:13:55.960; bot 0 sent its own at 55.971, and the server logged it at 56.086, 115 ms later, in tick 21; the new bot 1 process started in that window and joined at 56.108. On a runner of 4 cores, the new process's start shares them with the server and bot 0.

## Ready

