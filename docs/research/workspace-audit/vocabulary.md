# Workspace audit D: vocabulary

Read 2026-10-19 from `source/target/vocabulary.txt` (590 directories, 2,747 files, 2,436 Rust files; 4,769 types, 12,654 functions, 7,534 fields, 2,812 variants, 2,243 values), at `68ca71cbc`.

Method: each index entry tagged by side — **stack** (the 22 `crates/network/net-*`), **old** (`net`, `quic`), **Campfire** (everything else). Counts are names outside tests, benches and `internals` paths unless a line says otherwise; a name splits into words at `_` and at case changes. Test names come from the dump's layout (`#[test] fn`). Rules quoted are `AGENTS.md` (Direction, "One word, one meaning") and `02-engine-core.md` (Names).

Not checked: the meaning of every name. I read the doc comment of each type a finding names, and inferred the rest from the name and its file. Script-API names and package data keys (TOML, Rhai) appear here only where a Rust name carries them. I did not open the `.notes/ISSUES.md` log to see whether an item is already there.

## Proposed renames

| Current word or pattern | Where | Count | Proposed | Why |
| --- | --- | --- | --- | --- |
| `Session*` / `session` meaning the packet session | stack: net-packets, net-endpoint, net-payload, net-link, net-stepper, net-udp, net-replication, net-interpolation | 6 types, 7 fields, 3 fns, 1 variant, 5 files; 74 uses of "session" in `13-net-stack/` and 0 of "packet session" | `PacketSession*` / `packet_session`; `Lifetime::SessionBased` → `Lifetime::Link` | 12-lightyear-in-tree.md decided "its session is the packet session", but only the struct was renamed |
| Bevy plugin type names: bare (`Hud`, `SimServer`) against `*Plugin` | Campfire client (18), net (7) against stack (56) | 25 bare, 56 suffixed | `*Plugin`, groups `*Plugins` | Bevy's convention; a bare name already collides with a module (`Orders`) and a trait (`ActionNames`) |
| System sets `*Set` / `*Systems` / `*Stage` | sim and capabilities (19 `*Set`), client, net and stack (19 `*Systems`), stack (3 `*Stage`) | 41 | `*Systems` | Bevy 0.17's own rename, which `02-engine-core.md` already cites (`TransformSystems::Propagate`); `*Set` also names 15 collections (`TagSet`, `TeamSet`, `HookSet`) |
| `on_add_hook`, `on_remove_hook`, `on_insert_hook` | stack: net-messages, net-link | 13 | named for what they do, as `index_on_insert`; at least `on_add` / `on_remove` / `on_insert` | Names: "A hook is a script's"; the stack names one Bevy hook in three ways |
| `*Config` | stack, in 10 crates | 18 types, 48 names; Campfire 0 | `*Setup` (user's call: see D7) | Names: "a `*Setup` what a thing starts from"; Campfire has no `config` |
| `*Kind(TypeId)`: `ChannelKind`, `ComponentKind`, `MessageKind`, trait `TypeKind` | stack: net-channels, net-replication, net-messages, net-core | 4 types, about 20 names | `*TypeId`, trait `RegisteredType` | Names: "A kind is a member of a closed set"; these wrap a Rust `TypeId`, which is an open set |
| `PacketType` (×2) beside `PacketKind`, `TokenKind` | stack: net-packets, net-udp, net-channels | 2 | `*Kind`, qualified by layer (`HeaderKind`, `HandshakeKind`) | A closed set is a kind; `type` is a unit's or a script value's |
| `FragmentData`, `MessageData`, `SingleData` | stack: net-channels | 3 | `*Body` | Names: `*Data` is a section as a package writes it |
| `MemberSpec` | stack: net-interpolation | 1 | not `*Spec` (for example `MemberShare`) | Names: `*Spec` is a section with its names resolved |
| `VisualError<D>` | stack: net-prediction | 1 | `VisualOffset` | It is a decaying correction offset, not an error; Names: `*Error` is an expected failure |
| `*Context`, `Context*Fn` | stack: net-messages, net-interpolation, net-channels | 18 names | `*Cx` | Names: `Cx` is an engine context, `Ctx` the script's |
| `MessageId`, `NamedMessage(s)` for a localized text | Campfire: core/values, core/declare | 3 types, about 25 names | `TextId`, `NamedText(s)` | `Message` is now the network's (stack 91 names, Bevy's `Message`) |
| `sync` meaning "which clients receive a state" (`StateSync`, `SyncAll`, `SyncOwner`, `SyncTeam`) | Campfire: core/units, mode | 12 names | `audience` (`StateAudience`, …) | `sync` is also the stack's clock sync (36 names) and `store`'s fsync; `10-replication-audiences.md` already says "audience" |
| `Owners`, `InterpolationOwners`, `owns_*`, `refresh_*_owners` | stack: net-interpolation, net-replication | 11 | `holders` / `holds_*` | Names: "An owner is a player" |
| `EndpointParts` | stack: net-packets | 1 | `EndpointPieces` or the struct's fields by name | Names: "A part is a component an entity carries" |
| `PeerAddr` beside `LocalAddr`, `RemoteId`, `LocalId` | stack: net-link, net-connection | 2 types | `RemoteAddress`, `LocalAddress` | A pair is Local/Remote; whole words: `address` 26 against `addr` 7 in the stack, 0 `addr` in Campfire |
| `DisconnectedReason`, `StopReason`, `UnlinkReason` beside `LinkLossCause`, `DropCause` | stack: net-connection, net-payload; old net | 5 | `*Reason` for why a link or run ended, from a verb (`DisconnectReason`) | One meaning, two words; the stack pairs a participle with two verbs |
| `begin_*` / `finish_*`, `start_*` / `end_*` | stack: net-prediction, net-channels | `begin_rollback_check`/`finish_rollback_check`, `start_rollback`/`end_rollback`, `begin_send`/`finish_send` | `begin_*` / `end_*` | Names: "`begin` and `end` a span the caller drives … `finish` consumes a builder" |
| `max_message_size`, `fragment_size`, `min_payload_size`, `num_fragments`, `num_messages`, `num_bytes` | stack: net-channels, net-messages | about 30 | `*_len`; a count as the plural (`fragments`) | Names: "`len` a count of elements or bytes"; Campfire's `MAX_MESSAGE_LEN` |
| `*_secs`, `millis`, `nanos` | stack: net-prediction, net-interpolation, net-core, net-emulator | 20 | `_ms`, `_ns` (or one std-style family for all) | Campfire's 44 `_ms` names; old net's `_ns` |
| `maximum_*`, `minimum_*` | stack: net-sync, net-packets | 8 | `max_*`, `min_*` | The workspace's other 170 names use `max`/`min` |
| `rate: TickRate` (10) and `tick_rate: TickRate` (9) | Campfire sim, capabilities; stack | 19 fields | `tick_rate` | `rate` also holds `RateLimit` and a weapon's `Rate` |
| Bare-adjective predicates: `ended`, `settled`, `hidden`, `linked`, `predicted`, `overridden`, `public`, `forwardable`, `interpolated`, `rollback_needed`, `heartbeat_due`, `in_recovery` | stack | about 15 of 168 `-> bool` fns | `is_*` | Names: "A state is `is_<adjective>`" |
| `PriorityManager`, `MessageManager`, `PredictionManager` | stack | 3 types, 2 fields | named for what each holds | Campfire has no `Manager`; the word says nothing |
| `TickDelta`, `PositiveTickDelta` | stack: net-core | 2 | `TickSpan`, `PositiveTickSpan` | `delta` is Campfire's state change (`StateDelta`); 12-lightyear-in-tree.md calls it "a tick span" |

## Clashes: one word, two meanings

### D1. `session` still means the packet session in the stack — high

- Evidence: the rename stopped at the struct. Types: `CloseSession` (`net-packets/src/close_session.rs`), `SessionConfig` (`net-packets/src/session_config.rs`), `SessionCounts` (`net-packets/src/internals/session_counts.rs`), `SessionLink` and `SessionLinkObserver` (`net-endpoint/src/session_link.rs`, `endpoint_plugin.rs`), `SessionQueues` (`net-payload/src/session_queues.rs`). Fields `session: SessionConfig` in `EndpointConfig`, `Stepper`, `StepperConfig`, `UdpConfig`; `session: SessionQueues` in `Link`; `session: PacketSession` in `EndpointCore`. Functions `end_link_session` (`net-replication/src/controlled_by.rs`), `reset_session` (`net-interpolation/src/interpolation_timeline.rs`, which means "a new connection"). Variant `Lifetime::SessionBased` (`net-replication/src/lifetime.rs`, "despawned when the client disconnects"). Docs: `13-net-stack/` uses "session" 74 times (`session.md` 28, `udp.md` 15, `testing.md` 11) and "packet session" none.
- Campfire's `session` (46 names, in `protocol`, `runner`) and old net's (36) are a hosted match.
- Proposal: `PacketSession*` and `packet_session` everywhere the stack means its session; the docs say "packet session"; `Lifetime::SessionBased` → `Lifetime::Link`, `reset_session` → `reset_link`.

### D2. `hook` names Bevy component hooks in the stack — high

- Evidence: `on_add_hook` (6), `on_remove_hook` (6), `on_insert_hook` (1) in `net-messages/src/{event_sender,host_event_sender,host_message_sender,message_sender,message_receiver/mod}.rs` and `net-link/src/server.rs`. Beside them, the same hooks named `on_add` (12, `net-connection/src/connection_state.rs`, `server_state.rs`, `net-link`, `net-transport`, `net-prediction`, `net-replication`) and `on_insert` (3, `net-link/src/link_state.rs`). Campfire's own Bevy hooks are named for what they do: `index_on_insert`, `unindex_on_discard` (`sim/sim/src/stable_id/mod.rs`); old net `Viewer::enter_rooms`, `leave_rooms`.
- Rule: "A hook is a script's. An engine function the core calls for a module is a callback."
- Proposal: name each hook function for what it does; failing that, drop `_hook`. One style either way.

### D3. `sync` has three meanings — high

- Campfire script state: `StateSync`, `SyncAll`, `SyncNone`, `SyncOwner`, `SyncTeam`, `SyncCall`, `ModeSync`, `PlayerSync`, `SYNC: SyncTo` (`capabilities/core/units/src/unit_state.rs`, `mode/src/mode_state.rs`, `player_state.rs`): which clients receive a field. Campfire elsewhere calls the same thing an audience (`KindAudience`, `AUDIENCE`, 10 names) and sending (`SentOnce`, `SentOnChange`).
- `store`: `sync_dir`, `SlowSync`, `AppendError::Sync`, `DurableError::Sync`: fsync, as std's `sync_all`.
- Stack: the crate `net-sync`, `TimelineSync`, `SyncConfig`, `SyncSample`, `SyncSystems`, `is_synced` (36 names): clock and timeline sync.
- Proposal: `sync` for the timelines' sync and `store`'s fsync (both are the field's established terms); the script state's receivers become `audience` (`StateAudience`, `AudienceAll`, …), as `10-replication-audiences.md` names them. The script keyword `sync = "owner"` is public API: a decision for the user. Confidence: medium.

### D4. `kind` names a Rust type and an open set in the stack — high

- Evidence: `ChannelKind(pub TypeId)` (`net-channels/src/channel_kind.rs`, "The type of a registered Channel"), `ComponentKind(pub TypeId)` (`net-replication/src/component_kind.rs`), `MessageKind(TypeId)` (`net-messages/src/message_kind.rs`), trait `TypeKind: From<TypeId>` (`net-core/src/type_mapper.rs`); fields `channel_kind` (7), `kind_map` (3).
- Rule: "A kind is a member of a closed set." Campfire has 247 `kind` names in that sense (`KIND` 87, `DamageKind`, `EntityKind`).
- Also a synonym: `PacketType` twice (`net-packets/src/header.rs`, "What a session packet is, in the header's 3 type bits"; `net-udp/src/handshake_packet/mod.rs`) beside `PacketKind` (`net-channels/src/packet_kind.rs`) and `TokenKind`, `ClientKind`, `RollbackKind`, all closed sets.
- Proposal: `ChannelTypeId`, `ComponentTypeId`, `MessageTypeId`, trait `RegisteredType`; the two `PacketType`s become kinds named by layer (`HeaderKind`, `HandshakeKind`), as `PacketKind` is the channels'.

### D5. Campfire suffixes taken by the stack in another sense — high

- `*Data`: `FragmentData`, `MessageData`, `SingleData` (`net-channels/src/{fragment_data,message_data,single_data}.rs`), the bytes of a message on a channel. Campfire's 106 `*Data` types are a package section as written.
- `*Spec`: `MemberSpec` (`net-interpolation/src/interpolation_rule_component.rs`), "what a rule member owns of its component's histories". Campfire's 24 `*Spec` are resolved sections.
- `*Error`: `VisualError<D>` (`net-prediction/src/visual_correction.rs`), "One decaying correction", an offset; fields `error_margin`, `max_error_margin`, `previous_error_sign` (`net-sync`) are a sync error in the control sense. Campfire's `*Error` is an expected failure.
- Proposal: `*Body` for message bytes (`MessageBody`, `SingleBody`, `FragmentBody`), `MemberSpec` off `Spec`, `VisualOffset`. The sync controller's `error` is the established control-theory word: keep it in fields, never as a type suffix. Confidence: high for `*Data`/`*Spec`/`VisualError`, low for the fields.

### D6. `message` is two things — medium

- Campfire: `MessageId` (`capabilities/core/values/src/message_id.rs`, "Data names a message by it, and the package's `locale/` files give its text"), `NamedMessage`, `NamedMessages` (`core/declare/src/named_messages.rs`), `MAX_MESSAGE_LEN`: a localized text.
- Stack: `trait Message` (`net-messages/src/message.rs`, "Every type that can be sent over the network"), 91 names; old net's `InputMessage`; Bevy 0.17+'s own `Message`.
- Proposal: the locale side takes `text` (`TextId`, `NamedText`), since Bevy and the stack both mean a sent value. Fluent calls its unit a "message", so this is a trade: confidence medium.

### D7. Configuration: `*Config` against `*Setup` — medium (needs a decision)

- Stack: 18 `*Config` types (`AdmissionConfig`, `ChannelConfig`, `CompressionConfig`, `EndpointConfig`, `InputDelayConfig`, `InputTimelineConfig`, `InterpolationConfig`, `LinkEstimatorConfig`, `RemoteTimelineConfig`, `RollbackConfig`, `SessionConfig`, `StepperConfig`, `SyncConfig`, `TimelineSwitchConfig`, `UdpConfig`, traits `Config` ×2 and `TimelineConfig`), 48 names. Campfire: 0 `config`; `*Setup` 7 (`MatchSetup`, `ModeSetup`, `InProcessSetup`), old net 3 (`ServerSetup`, `LobbySetup`, `LocalServerSetup`); `*Params` 6 for script call parameters, plus the stack's `ServerParams` (`net-udp/src/transport_params.rs`, QUIC's "transport parameters"); `options` 13 (clap fields).
- Names: "a `*Setup` what a match, a mode or a server is built from".
- Options: (a) every stack `*Config` → `*Setup`, `ServerParams` → `ServerTransportSetup`: one word; but some are tuned while running (`recompute_input_delay_on_config_update`). (b) Add `*Config` to Names as "the tunables a running system reads", keep `*Setup` for what a thing is built from, and sort the 18 into the two. Pick (a) unless a type is changed at run time.

### D8. `checkpoint` is three things — medium

- Campfire: a session log checkpoint (29 names in `protocol`, `runner`: `Checkpoint`, `CheckpointError`, `begin_checkpoint`).
- Stack: Replicon's confirmed-update mark, mapped to net time (`CompletedCheckpoint`, `ReplicationCheckpointMap`, `CheckpointError`, `CheckpointFaults`, `last_confirmed_checkpoint`, in `net-replication`); and a rollback mark in a packet being built (`Checkpoint`, `net-channels/src/packet_builder/mod.rs`, "A state of an OpenPacket that an append can roll back to").
- `CheckpointError` is declared in both `session/protocol/src/session_log/error.rs` and `net-replication/src/checkpoint_error.rs`.
- Proposal: the replication one follows its source's word for what it marks, `confirmed update` (`ConfirmedUpdate`, `ConfirmedUpdateMap`); the packet builder's is a `Mark`. Confidence: medium; Replicon itself says "checkpoint"? not checked.

### D9. `owner`, `part` and `host` in the stack — medium

- `owner`: `InterpolationOwners`, `Owners` (`net-interpolation/src/interpolation_owners.rs`, "The owners of one pipeline's archetypes, those that hold its marker"), `refresh_owners`, `refresh_delayed_owners`, `refresh_frame_owners`, `owns_interpolation_history`, `owns_frame_history`, `owns_replicate`, fields `owns_apply`, `owns_history`: 11 names. Names: "An owner is a player". Proposal: `holders` / `holds_*`, the word Names gives a struct that holds a worker.
- `part`: `EndpointParts` (`net-packets/src/endpoint_parts.rs`). Names: a part is a component. Proposal: another word, or the fields by name.
- `host`: Campfire's `SimComponent::Host` (69, the entity kinds that carry a component), `ScriptHost`, a player who hosts a match; the stack's host-client and host-server (43: `HostClient`, `HostPlugin`, `HostMarker`, `is_host_server`). Proposal: decide in Names which one `host` is; the stack's is Lightyear's "host-server" mode, the usual "listen server". Confidence: low.

### D10. Same type name, different meaning, across the stack and Campfire — medium

The type names the stack shares with Campfire or old net, with their meanings (path as the dump gives it):

| Name | Campfire | Stack |
| --- | --- | --- |
| `Direction` | `sim/math/src/direction/mod.rs`, a geometric direction | `net-core/src/direction.rs`, client-to-server or back |
| `Replication` | `core/state-types/src/replication/mod.rs`, how a state type is sent | `net-replication/src/replication_kind.rs`, how a component replicates |
| `Packet` | `protocol/src/session_log/log_entries.rs`, a journal entry | `net-channels/src/packet/mod.rs` |
| `Receipt` | `protocol/src/receipt/mod.rs`, a signed result receipt | `net-channels/src/channel_receiver/mod.rs`, "Whether a received message was stored" |
| `Arena` | `test/harness/src/arena.rs`, a test match's arena | `net-common/src/arena/mod.rs`, a byte arena |
| `Clock` | `stats` modifier clock; old net OS clock | `net-timeline/src/clock.rs` |
| `Lifetime` | `stats/src/lifetime.rs` | `net-replication/src/lifetime.rs` |
| `Order` | `capabilities/src/order/mod.rs`, a unit's order | `net-stepper/src/stepper/mod.rs`, the order apps run in |
| `Delivery` | `actions/src/delivery.rs`, a projectile's delivery | `net-emulator/src/link_model_run/mod.rs`; `net-stack/src/scenario/wire_cost` |
| `Timer` | `mode/src/timers.rs` | `net-packets/src/endpoint_core/mod.rs` |
| `Side` | `navigation/src/regions/mod.rs` | `net-emulator/src/side.rs`, `net-packets/src/endpoint_core/mod.rs` |
| `Server` | `client/src/command_line/mod.rs` | `net-link/src/server.rs`, the server entity |
| `Instant` | std's | `net-core/src/instant.rs` |

Within the stack alone: `Unlinked` (`net-link/src/link_state.rs`, a link state; `net-replication/src/inheritance_plugin/mod.rs`, what an entity keeps of an aspect), `Endpoint` (`net-endpoint`; `net-messages/src/message_manager.rs`, a manager's list), `EndpointSide` ×2, `SentPacket`/`SentPackets` ×2 (`net-channels`, `net-packets`), `Admission` ×2, `Stop` ×2, `State` ×4.

Proposal: rename the one whose name is the narrower use: `NetDirection` or `Way` for the stack's (its `Directions` is "the ways a channel or message travels"), `ByteArena`, `StepOrder`, `MessageStored` for the channel `Receipt`, `WireDelivery`. Confidence: medium; each is a small rename.

### D11. `frame` has seven meanings — low

`core/script/src/scripts/frame/mod.rs` `Frame`, a script call's frame (Names uses this sense); `geometry/src/body_box` `Frame`, a coordinate frame; `protocol/src/journal/journal_frames.rs` `JournalFrames`, `FrameHash`, a framed record; `common/src/alloc_audit` `FrameKind`, a call-stack frame; old net `ServerFrame`, `ClientFrame`, `ServerFrameSystems`, an app update; stack `FrameInterpolation*` (33 names), a rendered frame, and `Frame` variants of `ClockSource` and `Reading`, an app update; `CryptoFrame`, `FrameSpan` (`net-udp/src/handshake_packet/mod.rs`), a QUIC-style packet frame. Proposal: Names picks two: the call frame and the app update (which Bevy and the stack's clock name); the journal's records become `records`, the geometric one a `basis`. Confidence: low, wide.

### D12. `visibility`, `room` and `audience` — low

Campfire's `visibility` crate is "a session's visibility: who receives each kind of state" (`core/visibility/src/lib.rs`); its per-kind rule is an `audience`. The stack's `visibility` (26 names, `NetworkVisibilityPlugin`, `gain_visibility`, `VisibilityBits`) is one entity's visibility to one link, grouped by `Room` (14 names); old net already uses rooms (`Viewer::enter_rooms`). Campfire's `room` otherwise means space (`has_room`, `make_room`, 10). The two visibilities are one idea at two grains, so no rename; Names could say so.

## Synonyms: one meaning, several words

### D13. Bevy plugins: bare names against `*Plugin` — high (needs a decision)

- Campfire: 18 client plugins with bare names (`ActionNames`, `AreaView`, `Bot`, `BuildPreview`, `ClientFiles`, `CommandPanel`, `Hud`, `InventoryBar`, `ItemNames`, `LinkWatch`, `LocalKeys`, `Orders`, `PlayerCamera`, `Selecting`, `ShopPanel`, `TerrainView`, `UnitModels`, `View`, in `apps/client/src`); old net 6 (`LocalPace`, `LocalRelink`, `NetProtocol`, `OneThread`, `SimClient`, `SimServer`) and the group `SimServerPlugins`. Campfire's sim modules are bare too (`Combat`, `Orders`, `Units`), but they implement `Module`.
- Stack: 56 plugins, every one `*Plugin` (`EndpointPlugin<Io>`, `LinkPlugin`, `MessagePlugin`, …), groups `ClientPlugins`, `ServerPlugins`, `ReplicationPlugins`; `SharedPlugin` beside `SharedPluginsBuilt`.
- Collisions now: the client plugin `Orders` (`apps/client/src/orders.rs`) and the module `Orders` (crate `orders`); the client plugin `ActionNames` (`apps/client/src/action_names.rs`) and the trait `ActionNames` (`core/actions/src/action_names.rs`).
- Proposal: every Bevy plugin `*Plugin`, every group `*Plugins`, as Bevy names its own; a bare capability name stays a Campfire `Module`. The suffix then tells a plugin from a module, which "One word, one meaning" lists as two words.

### D14. System sets: `*Set`, `*Systems`, `*Stage` — high

- 41 types derive `SystemSet`: 19 end in `Set` (`AbilitiesSet` … `VisionSet`, `SimSet`, `NoSet`, `StraySet`, in sim and capabilities); 19 in `Systems` (client `DrawSystems`, `OrderSystems`, `SelectionSystems`, `ViewSystems`; old net `ServerFrameSystems`, `SimTickSystems`; stack `ConnectionSystems`, `LinkSystems`, `MessageSystems`, `ReplicationSystems`, `PredictionSystems`, `RollbackSystems`, `SyncSystems`, `TimelineSystems`, `TransportSystems`, `InterpolationSystems`, `FrameInterpolationSystems`, `NetworkTopologySystems`, `PreSpawnedSystems`); 3 in `Stage` (`FrameStage`, `InterpolationStage`, `RollbackCheckStage`); others `Inner`, `ServerOnly`, `Shows`, `SimEdge`, `StageSpan`, `TickEnd`.
- `*Set` also names 15 collections (`TagSet`, `TeamSet`, `RoleSet`, `TrackSet`, `ResourceSet`, `HookSet`, `CapabilitySet`, `RelationSet`, `HoldSet`, `ConditionSet`, `ArmorSet`, `WeaponSet`, `FlagSet`), and `declare` has two traits named `Set` (`section/mod.rs`, `module.rs`).
- Proposal: `*Systems` for every system set, as Bevy 0.17 renamed its own and `02-engine-core.md` cites; `*Set` stays a collection; `ModuleSet` → `ModuleSystems`. A crate's sets take the crate's name, which `MessageSystems` (`net-messages`) and `DeliverySet` (`deliveries`) break.

### D15. The two ends of a link: `peer`, `remote`, `side`, `role` — medium

- Stack: `PeerId` (the id of either end), `RemoteId(PeerId)` and `LocalId(PeerId)` (`net-connection`), but `PeerAddr` and `LocalAddr` (`net-link/src/peer_addr.rs`, `local_addr.rs`); `Peer`, `PeerKey`, `PeerSet`, `peer_map`; `RemoteTimeline`, `RemoteEntityMap`, `RemoteEvent`, `ControlledByRemote`, `ExcessiveRemotePeers`. Client against server is `EndpointSide` (×2), `Side` (×2), `ClientSide`/`ServerSide` (`net-udp`) and `LinkRole` (`net-connection`). Campfire's `role` is a script role (33 names).
- Proposal: `remote` for the other end and `local` for this one (`RemoteAddress`, `LocalAddress`); `peer` for an end in general (`PeerId`); `side` for client-or-server (`LinkRole` → `LinkSide`).

### D16. Why something ended: `reason` and `cause` — medium

`DisconnectedReason` (`net-connection/src/connection_state.rs`), `StopReason` (`server_state.rs`), `UnlinkReason` (`net-payload/src/unlink_reason.rs`), Campfire's `LeaveReason`, `SkipReason` (protocol, import); old net's `LinkLossCause`, `DropCause`; Campfire's `DamageCause`, `HealCause` (combat: the source of an effect). Proposal: `*Reason` for why a link, a run or a seat ended, named from the verb (`DisconnectReason`); `*Cause` stays the source of a game effect.

### D17. Lifecycle verbs in the stack — medium

`begin_rollback_check` / `finish_rollback_check`, `start_rollback` / `end_rollback` (`net-prediction/src/rollback_plugin/mod.rs`); `PacketBuilder::begin_send` / `ChannelSender::finish_send` / `commit_send` (`net-channels`); `PortMap::start` / `PortMapper::end` (`net-port-map`). Names: `begin`/`end` a span the caller drives, `start`/`stop` what runs on its own, `finish` consumes a builder. Proposal: `begin_rollback_check`/`end_rollback_check`, `begin_rollback`/`end_rollback`, `begin_send`/`end_send`; `commit_send` stays if a commit is distinct from the end.

### D18. Byte counts, element counts and units — medium

- Size against len: stack `max_message_size` (6), `fragment_size` (5), `min_payload_size`, `max_decompressed_payload_size`, `DEFAULT_MAX_MESSAGE_SIZE`, `FRAGMENT_SIZE` (29 `size` names in all, 25 in `net-channels`) beside its own `bytes_len` (20), `payload_len`, `HEADER_LEN`, and Campfire's `MAX_MESSAGE_LEN`. Names: "`len` a count of elements or bytes".
- `num_*`: `num_fragments` (6), `num_messages` (2), `num_bytes`, `num_received_fragments`, where Campfire names a count by the plural of what it counts.
- Units: stack `_secs` (13: `decay_period_secs`, `duration_secs`, `elapsed_secs`), `millis` (2), `nanos` (5: `NANOS_PER_SEC`); Campfire `_ms` (44, data keys among them: `duration_ms`, `windup_ms`); old net `_ns` (2).
- `maximum_*`/`minimum_*` (8, `net-sync`) beside `max`/`min` everywhere else (about 170).
- Proposal: `*_len` for byte and element lengths, plurals for counts, `max`/`min`, and one unit-suffix family (Campfire's `_ms`, `_ns`). Confidence: high for `len`, `max`/`min`; medium for units, as `_ms` is in package data.

### D19. A `TickRate` field: `rate` against `tick_rate` — low

`rate: TickRate` in 10 Campfire structs (`MatchSetup`, `BookSource`, `TickClock`, …) and `tick_rate: TickRate` in 9 stack structs (`ClientPlugins`, `StepperConfig`, `TimelinePlugin`, …). `rate` also holds `RateLimit` (`LinkModel`, `Faults`) and combat's `Rate`. Data and terms keep `tick_hz: NonZeroU32` (11), a whole-hertz number. Proposal: `tick_rate`.

### D20. Context: `*Context` against `Cx` — medium

Stack `FragmentContext`, `InterpolationSampleContext`, `MessageReceiveContext`, `UpdateHistoryContext`, `ContextSerializeFn(s)`, `ContextDeserializeFn(s)`, `IntoContextLerpFn`, `with_context`, `interpolate_with_context`: 18 names. Names: "`Ctx`, the script's `ctx`, and `Cx`, an engine context". Proposal: `*Cx`.

### D21. Other synonyms — low

- `*Manager` (3: `MessageManager` "which lists a sender or receiver goes in", `PredictionManager`, `PriorityManager` "scheduler scratch"): name each for what it holds; Campfire has none.
- `*Fns` (17 structs) and `*Fn` (17 aliases), registered function tables: Campfire's word for an engine function called per module is `callback` (`CallbackTable`, `SpawnCallback`), and it uses `*Fn` for aliases too (`BuildFn`, `RuleFn`). Keeping `*Fns` matches Replicon's own; no rename proposed.
- `TickDelta`, `PositiveTickDelta` (`net-core/src/tick_time/mod.rs`) against `StateDelta` (sim) and "tick span" in 12-lightyear-in-tree.md: `TickSpan`.
- `handshake` (12), `Establish` (3), `hello` (7), `connect` (14) in the stack name separate layers' greetings; no rename, but Names could fix the layers' words.
- `stack` means a layer stack (`net-stack`, `ConnectionStack`, `FullStack`, trait `Stack`) and an item or modifier stack (`ItemStack`, `max_stacks`, `stack_life`); low.

## Conventions

### D22. Predicates — medium

Stack `-> bool` functions: 168, of which 71 open with `is_`; about 15 are bare adjectives or participles: `ended` (6 impls, `trait Establish`), `settled`, `hidden`, `linked`, `predicted`, `overridden`, `public`, `forwardable`, `interpolated`, `rollback_needed`, `heartbeat_due`, `in_recovery`, `synced_and_connected`. Names: "A state is `is_<adjective>`". Proposal: `is_*`.

### D23. Test names — medium

Of 1,250 Campfire tests, 1,161 (93 %) are sentences with an article (`a_tail_reads_each_line_once_as_its_file_grows`); old net 143 of 145. Of the stack's 900, 582 (65 %): the other 318, mostly `net-prediction` (74), `net-interpolation` (64), `net-replication` (61), `net-channels` (37), are the fork's telegraphic style: `swarm`, `both_timelines_sync`, `prediction_updates` (`net-stack/tests/alloc/fixtures/prediction.rs`), `predicted_modify_corrected_on_rollback`, `direct_root_write_does_not_mark`. Proposal: rewrite the 318 as sentences in Campfire's style. No test file uses a `test_` prefix.

### D24. `*Ext` against `extension` — low

The stack's 9 extension traits (`AppChannelExt`, `AppComponentExt`, `AppEventExt`, `AppMessageExt`, `AppInterpolationExt`, `InterpolationRegistrationExt`, `PredictionRegistrationExt`, `PredictionDespawnCommandsExt`, `VisibilityExt`) abbreviate "extension", which "One word, one meaning" keeps for a call extension (`CallExtension`, 23 names). `*Ext` is the Rust and Bevy idiom; keep it, and say so in Names.

### D25. Duplicates waiting on planned merges — fact

Decided in 12-lightyear-in-tree.md and not yet done: `TickRate` twice (`sim/sim/src/tick_rate/mod.rs`, `net-core/src/tick_rate.rs`; planned: one in `campfire-common`), `SplitMix64` twice (`sim/math`, `net-common`), `ChannelBinding` twice (`base/common`, `net-packets`), `LinkModel` twice (`test/harness`, `net-emulator`). `quic` and `net-port-map` share 40 type names (`PortMap`, `Gateway`, `UpnpGateway`, `NatPmpMap`, `PcpMap`, `ServerKey`, `Spki`, `UdpPort`, …) until `quic` goes.

## Layout

### D26. Files that hold no type of their name — medium

33 stack files hold no type named after them (Campfire: 70 of 1,342; old net: 1 of 156). Among them: `net-connection/src/connection_state.rs` (`Connected`, `Connecting`, `Disconnected`, `Disconnecting`, `DisconnectedReason`), `server_state.rs` (`Started`, `Starting`, `Stopped`, `Stopping`, `StopReason`), `net-link/src/link_state.rs` (`Linked`, `Linking`, `Unlinked`), `net-replication/src/replication_kind.rs` (`Replication`, `FullReplicated`, `DiffReplicated`, `NotReplicated`), `net-udp/src/transport_params.rs` (`ServerParams`), `net-udp/src/sockets/mod.rs` (`ClientSocket`, `ServerSocket`, `EmulatedServer`), `net-udp/src/address_token.rs` (`CheckedToken`, `TokenKey`, `TokenKind`), `net-core/src/tick_time/mod.rs` (`TickInstant`, `TickDelta`, `PositiveTickDelta`), `net-core/src/collections.rs` (`HashMap`), `net-interpolation/src/history_seed.rs` (`Seed`), `net-prediction/src/misprediction.rs` (`CheckTarget`). Several files declare 7 to 11 types (`net-channels/src/packet_builder/mod.rs` 11, `net-replication/src/inheritance_plugin/mod.rs` 9, `net-channels/src/fragment_receiver/mod.rs` 9), as Campfire's largest do.

### D27. `internals/` directories — medium

Ten stack crates hold a `src/internals/` directory of files (`net-channels`, `net-common`, `net-interpolation`, `net-messages`, `net-packets`, `net-prediction`, `net-replication`, `net-stack`, `net-sync`, `net-transport`), with names like `internals/protocol.rs`, `internals/steps.rs`, `internals/*_stack.rs`. No Campfire crate has one: Campfire's rule puts each gated helper in a `mod internals` at the end of the file whose privates it reads, "no aggregator".

### D28. Crate names — fact, low

Every crate directory matches its package (`campfire-<dir>`). The stack's crate names mix forms, as Campfire's do: plural `net-bytes`, `net-channels`, `net-messages`, `net-packets`; singular `net-connection`, `net-endpoint`, `net-link`, `net-payload`, `net-timeline`; mass nouns `net-interpolation`, `net-prediction`, `net-replication`, `net-transport`; a verb `net-despawn`; agents `net-emulator`, `net-stepper`. `net-common` and `net-core` repeat Campfire's `common` and `capabilities/core`; 12-lightyear-in-tree.md decided `net-common`. No rename proposed; Names could state the crate-name form.

## Leftovers of the fork

### D29. Words that name Lightyear, its crates or what Campfire refuses — high

- `clippy.toml` and the crate `clippy.toml`s of `net-despawn`, `net-interpolation`, `net-prediction`, `net-replication`: 16 bans each (80 lines) on `lightyear_messages::…` and `lightyear_replication::…` paths, crates that no longer exist, with the comment "each registers with Lightyear's own encoding". 12-lightyear-in-tree.md plans their removal ("the 18 bans … go").
- Links to `github.com/cBournhonesque/lightyear/issues/…`: 8, in `net-prediction/src/prediction_despawn_command/mod.rs`, `prediction_marker_plugin/tests.rs` (2), `rollback_plugin/tests/mod.rs`, `net-replication/src/network_visibility_plugin/tests.rs` (2), `net-stack/src/shared_plugin/tests.rs` (2). They cite upstream bugs; keep them as sources if wanted, but they are the only places code names Lightyear.
- WASM: `net-link/src/peer_addr.rs:6` and `local_addr.rs:6`, "An IO backend without OS sockets, such as one on WASM, does not insert it"; AGENTS.md: "no browser and no WebAssembly".
- `RawClient`, `RawServer`, `RawClientPlugin`, `RawServerPlugin`, `PeerId::Raw` (`net-connection`, `net-core`): "raw" is Lightyear's word for a connection without its netcode layer, which Campfire does not have.
- Docs: `13-net-stack/api.md:43` names `EndpointPlugin<SimWires>` (now `EmulatedWires`); `testing.md` names `CrossbeamIo`; "simulat…" 19 times, most of them the network or wire the code now calls emulated (`session.md`, `testing.md`, `api.md`); "session" for the packet session 74 times (D1).
- `Replicon*` (11 names: `ClientRepliconPlugin`, `RepliconChannelMap`, `RepliconMapParam`, …) name a dependency the stack still uses, not the fork: no change.
- `net-sync/src/input_timeline_config.rs:46` cites Netcode for Entities as a precedent: fine.

## Campfire-internal, still open from the earlier audit

### D30. `*File` has no recorded meaning — low

`AGENTS.md` lists `*File` among the words of one meaning; `02-engine-core.md`'s Names gives `*Data`, `*Spec`, `*Book`, `*Decl`, `*Setup` and not `*File`. 27 `*File` types: a package file (`ModeFile`, `UnitsFile`, `MapFile` in `package`), a client file (`HudFile`, `KeysFile`, `TerrainFile`), how `store` opens a file (`DurableFile`, `InputFile`, `OutputFile`, `PinnedFile`, `SecretFile`, `AppendFile`), another game's format (`DdsFile`, `TgaFile`, `W3dFile`, import's `MapFile` and `UnitsFile`), `LogFile`, `RepoFile`, `SourceFile`, `HashedFile`, old net's `KeyFile`, `SeatBotFile`. `MapFile` and `UnitsFile` are each declared twice (`import`, `package`). Proposal: Names records `*File`'s meaning (a whole file of one format), and import's two collisions take the game's name (`ZeroHourMapFile`) or move into its `zero_hour` module's names.

### D31. Type names declared twice inside Campfire — low

About 50 Campfire type names are declared in two or more crates, among them `RowReads` (8 crates), `Entry` (8), `CommandLine` (7, decided), `Held` (5), `Share` (4), `Shape` (4), `Args` (4), `Slot` (3: `store`'s and two in core), `Pass` (3), `View`, `Units`, `Unit`, `UnitTypes`, `Terrain`, `Turn`. Most are private helpers; `Slot` in `store` (`latest_writer`) still clashes with Names' "slot is a unit's".
