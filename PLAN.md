# Plan

The one plan of the project: every open step, in the order it is done, in batches. A batch is one commit: it holds the steps that change the same systems, refactorings, fixes and features alike, and ends with `./drive.py check` passing, and a stop for review. Each batch says what it touches and why it stands where it does. Open items only: a step goes when it is done. The issue log, `.notes/ISSUES.md`, holds every open problem; a problem that a step here takes carries the step's tag there ([Issue log](AGENTS.md#issue-log)). The designs in `docs/design/` stay the source of truth: a step names what the code lacks against its design, or the design it proposes, which moves into `docs/design/` when it is agreed.

Every batch moves the engine toward the [Direction](AGENTS.md#direction), and each step names the part it builds. Line numbers point to the code as each step was written; a step refreshes its references as it starts. A step that states no check holds this one: no golden moves, and `./drive.py check` passes.

## 1. The network

`quic`, `net`.

### N1. Limits for each address

(`quic`, `net`; **Quic**.) No limit holds live connections or new handshakes for each address, where design 15's D13 bounds both by the session's seats for each IPv4 address or IPv6 /64, and `net` gives the endpoint no seat count ([QUIC transport](docs/design/15-quic-transport.md), D13). After the bench of the endpoint, the issue log's **Quic** item on benches, sets the cap of new connections on the same path.

### N2. UPnP behind a host firewall

(`quic`; **Quic**.) `igd-next` discovers a router by search alone, and the router's `NOTIFY` announcements, which a host firewall that tracks connections lets in, go unread ([QUIC transport](docs/design/15-quic-transport.md), D12).

Then the network's Research items of the issue log as they show.

## Foundations

The open problems whose cause is in the architecture or in a format the project chose, not in one piece of code: each shows as several items of the issue log, or limits every system built after it. Each entry names its cause, what it limits, and the issue-log items it holds ([Issue log](AGENTS.md#issue-log)).

- **The import's materials are glTF's** (Zero Hour). glTF's material model holds no W3D pass past the first, no ambient color and no fallback texture, so the presentation loses each, and each later fidelity gap of a W3D material meets the same limit. Items: Zero Hour, "The import writes W3D's materials into glTF's material model"; Zero Hour, "A texture no archive holds"; Zero Hour, "The import ignores a vertex material's ambient color"; Zero Hour, "The import draws a mesh's first material pass".
- **The import has no object that only the client shows** (Zero Hour). Every object a map places becomes a unit of the sim, where the game spawns props, optimized trees and fluff on the client alone and some objects not at all, so the sim, its log and its state hash hold objects the original never simulates, and the parity oracle meets them. Items: Zero Hour, "The import makes a unit of every object a map places".
