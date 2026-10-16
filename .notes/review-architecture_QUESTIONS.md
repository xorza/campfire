# Questions of the architecture plan

Decisions that the plan in `review-architecture.md` cannot take alone. Each names its item, the options, the recommendation and what waits for it.

## Q1. Batch 1.1, transport bounds: where the fix goes

- **Item:** 1.1, N-1 and N-2. Lightyear's fragment receiver allocates `num_fragments × fragment_size` from the peer's count before any check, and a link that did not join uses every reliable channel with no budget (fork `xorza/lightyear` at `54e6be2`, `crates/transport/transport/src/channel/fragments/receive.rs:137-147`, `channel/receive.rs:179-215`).
- **Why it waits:** the fix belongs in the fork's transport crate, a repository apart from this one. This plan may commit and push only on its own branch of `campfire`.
- **Options:**
  1. **Fix the fork (recommended).** A branch of `xorza/lightyear` gives each channel a largest message size, refuses a larger fragment count before it allocates, caps the half-built messages for each link and channel, and expires them on reliable channels too, as ENet and yojimbo check before they allocate. `net_protocol` declares each message's largest encoding, and this repository pins the fork by `rev`. Cost: a change in the fork, which the user commits and pushes, or allows.
  2. **A guard in `quic` or `net`.** It reads Lightyear's packet header from each datagram before Lightyear does, and drops one whose fragment count passes a bound. Cost: a second reader of Lightyear's wire format, which breaks when the fork changes it.
  3. **Vendor the transport crate.** Copy `lightyear_transport` into the workspace as a path patch and fix it there. Cost: a copy of an outside crate to keep in step with the fork.
- **Blocked:** batch 1.1 alone. No later batch needs it.
