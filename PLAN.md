# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## 1. The network

`quic`, `net`.

### N1. Limits for each address

(`quic`, `net`; **Quic**.) No limit holds live connections or new handshakes for each address, where design 15's D13 bounds both by the session's seats for each IPv4 address or IPv6 /64, and `net` gives the endpoint no seat count ([QUIC transport](docs/design/15-quic-transport.md), D13). It comes after the issue log's **Quic** item on benches, whose bench of a flood also sets how many new connections a pass takes (D14).

### N2. UPnP behind a host firewall

(`quic`; **Quic**.) `igd-next` discovers a router by search alone, and the router's `NOTIFY` announcements, which a host firewall that tracks connections lets in, go unread ([QUIC transport](docs/design/15-quic-transport.md), D12).

## Foundations

- **The import's materials are glTF's** (Zero Hour). The import writes only what glTF's material holds, one pass, no ambient color apart from the diffuse one and no fallback texture, so the presentation loses each, and each later fidelity gap of a W3D material meets the same limit; design 12's D13 puts each in the material file, which holds no ambient color yet. Items: Zero Hour, "The import writes no material file for a vertex material whose ambient color"; Zero Hour, "A texture no archive holds"; Zero Hour, "The import draws a mesh's first material pass".
- **The import has no object that only the client shows** (Zero Hour). Every object a map places becomes a unit of the sim, where the game spawns props, optimized trees and fluff on the client alone and some objects not at all, so the sim, its log and its state hash hold objects the original never simulates, and the parity oracle meets them; design 12's D14 gives them a client file of the map, which no code writes or reads yet. Items: Zero Hour, "The import makes a unit of every object a map places".
