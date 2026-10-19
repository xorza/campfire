# Workspace audit

Read on 2026-10-19 at commit `68ca71cbc`, the commit that copied the network stack into the tree. Five audits read the workspace in parallel, and each wrote its findings, with their evidence, to one file of [workspace-audit/](workspace-audit/). No audit built, ran or changed code. The decisions that these facts lead to are in [Workspace alignment](../design/14-workspace-alignment.md), and the plan's batches in [PLAN.md](../../PLAN.md).

| File | Scope | Findings |
| --- | --- | --- |
| [campfire-crates.md](workspace-audit/campfire-crates.md) | The 50 crates outside the network: kind, responsibility, dependencies, layers, layout | A1 to A25 |
| [net-stack.md](workspace-audit/net-stack.md) | The 22 `net-*` crates, `net` and `quic`: dependencies, boundaries, layers, Campfire's conventions, the install of systems, tests | B1 to B25 |
| [consolidation.md](workspace-audit/consolidation.md) | The concepts that Campfire and the stack both implement, and what of `net` and `quic` survives the move onto the stack | C1 to C23 |
| [vocabulary.md](workspace-audit/vocabulary.md) | Every declared name and the layout, from `cargo drive run vocabulary` | D1 to D31 |
| [issues.md](workspace-audit/issues.md) | Each item of `.notes/ISSUES.md` and of the plan's Foundations, against the code | E1 to E42, F1 |

Each ID is unique across the five files. The files name the plan's batches by their titles, as the plan's numbers change.

## Checked by hand

These claims were read again in the code after the audits:

- **The two tick rates are two resources today.** `sim`'s `TickRate` derives `Resource` (`sim/sim/src/tick_rate/mod.rs:9`), and `net-core`'s does under `ecs` (`net-core/src/tick_rate.rs:18`). `SetTickRate` writes the stack's resource (`net-timeline/src/local_timeline_plugin.rs:24-25`), and the client builds the sim in its app's world. So one merged type that both sides read as one resource gives a sped-up local match a second rate in its sim (C1).
- **The workspace's Bevy turns on `multi_threaded`** (`source/Cargo.toml:220`), and the stack's allocation audit asserts one thread in each task pool (`net-stack/tests/alloc/audit/mod.rs:106-114`). The stack builds Bevy 0.20 apart from Campfire's 0.19 today, so the two meet first when Campfire moves onto the stack (C10).
- **`net-bytes` names `codec` and `serde` only for its serde bridge** (`writer/mod.rs:9-11`, `reader.rs:4-6`, `serialization_error.rs:3`) (B1).
- **`net-transport` requires `bevy_utils`, and names it in a doc comment and in `internals` alone** (`channel_registry.rs:23`, `internals/test_transport_plugin.rs:5`). The rule of required dependencies passes it, as `SourceFile::production` keeps comments (B2).
- **`net-link` names `net-timeline` once, to add `ClockPlugin`** (`link_plugin.rs:6`) (B5).
- **`TransformDiff`, `TransformDiff2d` and `transform_lerp` have no caller** outside their own files and the crate roots (B6).
- **`net-prediction` takes the rule registry and frame interpolation from `net-interpolation`**: `HistoryComponent` 9 times, `InterpolationRegistry` 5, `FrameInterpolationPlugin` 3 (B9).
- **26 production sites of the stack call `is_plugin_added`** (B20).
- **`net-channels/src/reliable_send_state.rs` holds 11 `unreachable!`**, the server's side of the handshake implements `connect` as `unreachable!` (`net-udp/src/secure_establishment/server_side.rs:428`), and `PortMapError::NoGateway` holds a `String` (`net-port-map/src/error.rs:13`) (B19).
- **A header decode accepts two encodings of one value**: an `Option` reads any nonzero tag as `Some` (`net-bytes/src/to_bytes.rs:53`), and a varint reads a length longer than its value needs (`net-bytes/src/reader.rs:135-143`) (C18).
- **`values` names the ECS in six files** (`team.rs`, `unit_type.rs`, `unit_tags.rs`, `relations.rs`, `hit.rs`, `dash_delivery.rs`), and **`visibility` names `state-types` for `DataKind` and `Recipients` alone** (A2, A4).
- **`harness` requires `areas`, `projectiles` and `state-types`** in `[dependencies]` (A8).
- **The stack still names the packet session "session"**: `SessionConfig` 46 times, `SessionQueues` 39, `SessionCounts` 10, `CloseSession` 9, `SessionLink` 8, against `PacketSession` 18 (D1).
- **The test that issue E30 asks for exists**: `an_unchanged_sample_is_not_a_change` (`net-interpolation/src/interpolate/tests.rs:335`).
- **02 refuses a docs lane** ("No docs lane", `02-engine-core.md:315`), which `13-net-stack/testing.md` plans as a rustdoc lane (E33).
- **The `ship` step checks `net-channels`** (`target/drive/log/ship.log`), whose default features leave `compression_lz4` off; the log does not show the features it checked with, so the lint without lz4 that E33 asks for is likely held already, not confirmed.
- **Three of the issue log's Decide items are open**: E1, E2 and E3.

## Corrections

- **The UDP TLS label changes every published key.** `quic` derives the TLS seed by HKDF-SHA256 with no salt and the label `campfire/tls/v1` (`quic/src/derived_secret.rs:19`). `net-udp` derives it in the same way with `campfire/udp/tls/v1` (`net-udp/src/derived_secret.rs:20`), a label that batch 1 gave it. With `quic`'s label, the same key file gives the same `TlsKey` after the move onto the stack, so no listing's pin changes. C8 offers that as one option; it is the one that keeps the key.
- **Batch 1 named `MemberSpec`** (`net-interpolation/src/interpolation_rule_component.rs`), which uses `*Spec` in a sense that Names does not give it (D5).
- **E33's rustdoc lane conflicts with 02**, and the issue's lint without lz4 is likely the `ship` lint (above).

## Coverage

Each finding goes to a decision of [Workspace alignment](../design/14-workspace-alignment.md), named by its title, or is not taken, with the reason.

| Findings | Where | Note |
| --- | --- | --- |
| A1, B14 | Every crate declares its kind in its manifest; The kinds are an engine-wide decision of 02 | |
| A2, A3, A4, A23 | `values`, `visibility` and `client-files` are problem crates; `bevy_platform` and `bevy_math` are engine-free | |
| A5 | Not taken | 02's "Rhai, not `bevy_mod_scripting`" keeps the host in `script`, and no second user of it exists |
| A6 | Not taken | The package format has no reader without the release yet; the planned launcher is the first |
| A7 | The harness splits at the network | |
| A8, A14, A15 | A crate depends only on what its code names | |
| A9, B24 | One measurement crate | |
| A10, B2, E40 | `rules` reads Rust as Rust | |
| A11, A12 | A structural rule lives in `rules` | |
| A13 | Not taken | A pinned cross-check that the client cannot make, as it depends on no import |
| A16, D19 | A field of a tick rate is `tick_rate` | |
| A17 | `common` holds plain shared values alone | |
| A18 | A directory names a domain | |
| A19 | Not taken | The client's library waits until the client runs on the stack, and no decision names it yet |
| A20 | Not taken | The Zero Hour readers are one game's; a second importer splits them |
| A21, A25 | No decision | Facts of the stack's base edges and of `quic`'s copies, which its deletion removes |
| A22, B13 | Planned: the split of `net` | |
| A24, B7, B25, C15 to C17, C19 | The plan's batch "Campfire on the stack" | C17's line-by-line check of the port maps is a step of that batch |
| B1, E8 | `net-bytes` is the header codec alone | |
| B3, B11 | `net-common` goes | |
| B4, B8, C12 | The registry hash is a domain hash; One source of OS random bytes | |
| B5, B6 | No feature without a caller | |
| B9 | Planned: the split of interpolation | |
| B10 | Not taken | Replication's jobs meet in Replicon; receiver history is the one seam, weighed with B9 |
| B12 | The emulated wires are the packet session's IO | |
| B15, B18, D27 | One path for each item | |
| B16 | The stack's systems are methods | |
| B17 | The stack's documentation is Campfire's | |
| B19 | A state the code cannot reach is a type | |
| B20 | The stack stays on Bevy's plugins; A plugin group lists every layer | |
| B21 | The stepper runs Campfire's strict schedules | |
| B22, D10, D31, C23 | A type name that both sides declare keeps one meaning | |
| B23 | One rule of test homes; Planned: one driver of integrated tests | |
| C1 | One tick rate type, two resources | |
| C2 | The plan's batch "The stack on Campfire's base crates" | Its check adds that `Num` saturates where `fixed` did |
| C3, C14, C20 | No change | Each pair makes two promises |
| C4 | A seeded draw means one thing | |
| C5 | One source of OS random bytes | |
| C6 | The singleplayer link is sealed | |
| C7, C8 | One type of the server's TLS key; The UDP labels keep the published key | |
| C9 | One log check | |
| C10, C11, E29, E31 | One measurement crate | |
| C13 | The plan's batch "Campfire's glue on the stack"; `UnixClock` | |
| C18 | A header decodes from one encoding | |
| C21 | Not taken | A shared wake signal fixes no bug |
| C22 | The stack's host-client path goes | |
| D1, D2, D4, D5, D9, D15 to D18, D20 to D22, E32 | The stack's words that break Names change | |
| D3 | Script state's receivers are its audience | |
| D6 | A localized text is a text | |
| D7 | Settings are a `*Setup` | |
| D8 | Not taken | The replication checkpoint follows Replicon's word, which the audit did not read; the batch of the stack's words reads it first |
| D11 | Not taken | `frame` has seven senses at low confidence; Names already gives the call frame |
| D12, D28 | No change | The two visibilities are one idea at two grains; the crate names follow E3 |
| D13 | A Bevy plugin is a `*Plugin` | |
| D14 | A system set is `*Systems` | |
| D23 | A test name is a sentence | |
| D24, D30 | Names is the one table of words | |
| D25 | The plan's batches "The stack's boundaries" and "The stack on Campfire's base crates" | |
| D26 | Not taken | Files named after none of their types are at Campfire's rate, 0.23 for each 1,000 lines on both sides |
| D29 | Nothing names the fork | |
| E1, E2, E3, E38 | Open decisions; Campfire's words | |
| E24, E27 | One rule in one place | |
| E26 | A rule lives with its owner | |
| E30, E42 | The plan's batch "The checks that hold every crate" | E30 is fixed; E42 is a comment |
| E33 | The driver's lanes are 02's | |
| E10, E13 to E15, E34, E39, F1 | The plan's batch "Campfire on the stack" | The stack fixes each, so each goes with `quic`; two gaps stay as items of the stack: no scenario measures a client's join burst, and no bench loads the endpoint pass |
| E9 | The plan's batch "Campfire on the stack" | The LAN check runs again on the stack before the item is researched |
| E12, E35 to E37 | The plan's batch "Campfire's glue on the stack" | |
| E11 | Not taken | Bevy's task pools are global to a process, so two apps in one process share them on any stack |
| E4, E5, E6, E7, E16 to E23, E25, E28, E41 | Not taken | No crate boundary, dependency, shared tool or word causes them |
