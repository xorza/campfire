# Plan questions

## K2. Keep the TLS identity

A restored server must present the TLS certificate its clients pinned, so the server keeps its certificate and key in its data directory. Lightyear re-exports only wtransport's `Identity`: the types that build one from stored bytes (`Certificate`, `CertificateChain`, `PrivateKey`) are not reachable through it, and `Identity::load_pemfiles` is async on tokio. Keeping the identity needs a new manifest entry, which needs your approval.

Options:

1. **Add `wtransport = "=0.6.1"` to `campfire-server` (recommended).** It is in `Cargo.lock` already, through Lightyear, at that version, so nothing new is built. The server stores the certificate and key as DER (`tls/cert.der`, `tls/key.der`) with `Certificate::der` and `PrivateKey::secret_der`, reads them back with `Certificate::from_der` and `PrivateKey::from_der_pkcs8`, all synchronous, and keeps the expiry it made the certificate with in `tls/expires`, so no X.509 parser is needed.
2. **Add `aeronet_webtransport` instead,** which re-exports `wtransport`. The same code, through one more layer of re-export.
3. **Make a new certificate at each start.** No dependency, but a restored server then has a new certificate hash, and the clients that pinned the old one cannot reconnect: crash restore works only for clients started again with the new hash.

Blocked: K2. The LAN check's restore scenario in X1 needs it; every other step goes ahead, as in-process tests link with no TLS.
