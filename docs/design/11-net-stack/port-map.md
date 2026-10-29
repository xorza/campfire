# Port mapping

How `net-port-map` gets a player-hosted server's port forwarded by the home router in front
of it. It takes a bound port and knows nothing of the protocol on it. The facts behind it are
in [UDP research](../../research/udp.md#port-mapping).

- **A server maps its own port,** on a thread of its own. `PortMap` starts once the server's
  socket is bound, and holds no lock the frame takes. Its drop deletes the mapping in one bounded
  try and joins the thread, and each grant is held as the router makes it, so the drop deletes
  every mapping the router made. An app that hosts where no router answers forwards the port by
  hand.
- **PCP, then NAT-PMP, then UPnP-IGD.** Each in turn, with its own short tries, so the first that
  answers wins within seconds: PCP's `MAP` to the default gateway, which `netdev` names; NAT-PMP
  when the gateway answers PCP with version 0; UPnP-IGD last.
- **UPnP-IGD is the map's own SOAP, over `ureq` with plain HTTP alone.** It keeps `igd-next`'s
  practice: an action's arguments are the ones the router's schema names, a port drawn when the
  router lacks `AddAnyPortMapping`, and each request bounded at 1 MiB and 10 s; the draw comes from
  the mapper's `Random`. Refused: `igd-next` and its `attohttpc`, whose `url` brings ICU4X, 77
  crates on Linux against 44 (`cargo tree`, 2026-10-19); `minreq`, which expects a server that
  behaves, where a router's reply is untrusted; `portmapper` and `port-control-client`, which need
  Tokio.
- **A router is found by its search answer or by its announcement.** A host firewall that tracks
  connections drops an answer a router sends from a port other than 1900, as TP-Link's ER605
  does, and lets the multicast `NOTIFY` announcement in. So the map listens on port 1900 beside
  the host's other listeners, a socket with `SO_REUSEADDR`, and `SO_REUSEPORT` on Unix, through
  `socket2`, and builds a router from the description and the service schema at the location
  that an answer or an announcement names. Either is a router's only at a location of its
  sender's own address, as `igd-next` trusted an answer, and an announcement only when it says
  the gateway or its WAN connection is alive. Each document is read over HTTP, no redirect
  followed, within 1 MiB and an XML nesting of 64, as `igd-next` bounded its own, every URL of it
  at the router's address. An announcement of a router at an address not heard before, while no
  router maps the port, starts a turn at once, so a host that varies its location starts one turn
  at most; a turn whose search finds no router maps through the last router heard.
- **Only the gateway announces a router.** An announcement is taken only from the default
  gateway's IPv4 address, as each turn reads it from the OS: an announcement names the host the
  mapper then asks over HTTP, so one any host could send, from any address it makes up, would aim
  the mapper at hosts of its choice. Refused: a sender's address alone, which an off-path host
  forges.
- **A mapping asks for 7,200 s and renews at 1/2 to 5/8 of what it got,** as RFC 6887 §11.2.1 has
  it; a UPnP router that takes only a lease that never ends gets one, which the map still renews.
  A map that fails is tried again in 30 minutes, or, after a PCP refusal, once the refusal's
  lifetime ends, as RFC 6887 §7.2 has a client expect the same answer until then, and at most the
  7,200 s it asks for, as a router's refusal may last for years. A renewal that fails starts the
  turn again.
- **The address outside is logged, and a private one warns.** When the router's own address is
  private, of RFC 1918 or carrier-grade NAT's 100.64.0.0/10, another NAT stands before it: the map
  warns that a player outside joins only if that NAT forwards the port, and keeps the mapping, as
  a modem that forwards to the router makes it reachable.
- **IPv4 alone.** A host on IPv6 needs no translation, and its firewall's pinhole stays the
  host's; a server bound to loopback, or to an IPv6 address but the wildcard, maps nothing. The
  wildcard `[::]` takes IPv4 too (11-net-stack/udp.md, An IPv6 socket is dual-stack), so its IPv4 port is
  mapped as `0.0.0.0`'s is.
- **The app names the mapping,** which a UPnP router lists by that name.
- **The draws come from the mapper's `Random`, the clock from the OS.** A nonce must be unguessable
  (RFC 6887 §11.2), so a build draws from the OS, and a test from a seed, which fixes each nonce and
  renewal time it checks. The thread waits on sockets, so it keeps the OS clock ([A real-socket IO
  runs on `Os`](time.md)). `Random` is `net-core`'s, its OS source `getrandom`, the workspace's one.
  Refused: a copy of the type in each crate that draws.
