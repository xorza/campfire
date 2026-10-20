/// The largest datagram of a link whose IO names none: the 1,200 bytes every IPv6 path carries,
/// which the emulated network and the UDP protocol both keep (11-net-stack/udp.md, The MTU is a
/// fixed 1,200 bytes).
pub const DEFAULT_MTU: usize = 1200;
