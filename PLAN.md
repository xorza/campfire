# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Lightyear in the tree

The slice of [Lightyear in the tree](docs/design/12-lightyear-in-tree.md): the player hosts a LAN match and plays a singleplayer match as before, every link on Campfire's own network stack, copied from the fork's `main` into `source/crates/network/`, over its own UDP. With it go the batches of [Workspace alignment](docs/design/14-workspace-alignment.md), which give each crate one kind and one job, hold the stack to Campfire's conventions, give each shared tool one home and each word one meaning. The batches go by impact (Workspace alignment, The batches go by impact): first what is wrong now or makes each later batch safer, then the stack reshaped and renamed before Campfire names its types, then Campfire on the stack, then Campfire's own moves and words, which no batch of the slice needs. A batch never goes before one it needs. Each batch ends with `cargo drive gate` passing; a golden that moves names its batch and its cause.

### 13. The fold

`docs`: `02-engine-core.md` (Networking, the crates table), `05-protocol-spec.md`, `08-sessions.md`, `10-replication-audiences.md`, `04-capabilities/vision.md`, `11-quic-transport.md` and `docs/research/quic.md` (deleted). It stands last, as a slice ends when the user plays it.

- [ ] docs/design/05-protocol-spec.md:51 — the connect flow says a client connects over QUIC, which a third party implements with any QUIC library (The links run on the stack's UDP). → The spec names the stack's UDP: its handshake packets, its header and its seal, with their byte layouts and their one encoding, as a third party now implements Campfire's own protocol; the Retry and the limits per address keep their place in the flow.
- [ ] docs/research/quic.md — facts of a transport no decision takes any more. → Deleted, but each fact a decision of `13-net-stack/udp.md` cites, as `quinn-udp`'s socket, which moves into the stack's UDP research.
- [ ] docs/design/12-lightyear-in-tree.md — the slice's decisions stand in documents of their own (A slice's proposal is temporary). → Once the user plays a LAN match and a singleplayer match on the stack, each lasting decision of 12 and of `14-workspace-alignment.md` folds into 02, `13-net-stack/` or the document of the system it governs, the decisions of `11-quic-transport.md` that are Campfire's own into `08-sessions.md`, "Lightyear" and "QUIC" become the stack's names in each document and in each citation of the code, and designs 11, 12 and 14 are deleted, each citation of theirs in the code naming the decision's new home.

## Steps

**T1. The toolchain back to stable.** Touches `source/rust-toolchain.toml`. When Rust 1.100 is released, on 2026-11-12, the pin moves from the dated beta to stable 1.100, as its comment says.
- **Check:** `cargo drive gate` passes on the stable toolchain.

## Foundations

