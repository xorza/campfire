# The network stack

The `net-*` crates are Campfire's network stack. Their code comes from
[Lightyear](https://github.com/cBournhonesque/lightyear), by cBournhonesque, through the
fork [xorza/lightyear](https://github.com/xorza/lightyear), copied at its `main` commit
`98ec6345`. Campfire changes it as its own code from that commit on, and takes nothing from
either repository again.

Lightyear is licensed under MIT or Apache-2.0, at your option; `LICENSE-MIT` and
`LICENSE-APACHE` here are its texts. Campfire's own changes take Campfire's licenses, the same
two.

The stack's decisions are in [`docs/design/11-net-stack/`](../../../docs/design/11-net-stack/).
