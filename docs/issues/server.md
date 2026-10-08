# Server

Design: [Modules](../design/02-engine-core.md#modules), `server`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide


## Research

## Ready

- **Plan: F6.** `ServerTls::open` reads the data directory's `tls` file, which holds the TLS identity's private key, with `fs::read`, and checks no mode, where [Storage](../design/02-engine-core.md#storage) makes it a `SecretFile`, as `server.nsec` is, which refuses a file others may read.
