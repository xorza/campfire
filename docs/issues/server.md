# Server

Design: [Modules](../design/02-engine-core.md#modules), `server`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

- `ServerTls::open` reads the data directory's `tls` file, which holds the TLS identity's private key, with `fs::read`, and checks no mode, while `server.nsec` is read as a `SecretFile`, which refuses a file others may read. [Storage and workers](../design/11-storage.md) names the key file as the one secret file. The question: is `tls` a secret file, its mode checked as `server.nsec`'s is, or a file whose key the server key's handshake makes public enough to read as it is now?

## Research

## Ready
