use std::net::{Ipv4Addr, SocketAddrV4};

/// An HTTP URL of a router on IPv4, as a UPnP description gives one: its address, port 80 when
/// it names none, and its path with its query. A URL of a host name, of IPv6 or of another
/// scheme is none: a router on IPv4 names itself by its address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RouterUrl {
    addr: SocketAddrV4,
    path: String,
}

impl RouterUrl {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let scheme = text.get(..7)?;
        if !scheme.eq_ignore_ascii_case("http://") {
            return None;
        }
        let rest = &text[7..];
        let (authority, path) = rest.find('/').map_or((rest, "/"), |at| rest.split_at(at));
        let addr = match authority.rsplit_once(':') {
            Some((ip, port)) => SocketAddrV4::new(ip.parse().ok()?, port.parse().ok()?),
            None => SocketAddrV4::new(authority.parse::<Ipv4Addr>().ok()?, 80),
        };
        let path = path.split('#').next().unwrap_or("/");
        (addr.port() != 0).then(|| Self {
            addr,
            path: path.to_owned(),
        })
    }

    /// The URL `reference` names, read against this one: a whole URL, a path from the root, or a
    /// path beside this one's.
    pub(crate) fn join(&self, reference: &str) -> Option<Self> {
        let reference = reference.trim();
        if reference.is_empty() || reference.starts_with("//") {
            return None;
        }
        if reference.starts_with('/') {
            return Some(Self {
                addr: self.addr,
                path: reference.to_owned(),
            });
        }
        if reference.contains("://") {
            return Self::parse(reference);
        }
        let directory = self.path.rfind('/').map_or("/", |at| &self.path[..=at]);
        Some(Self {
            addr: self.addr,
            path: format!("{directory}{reference}"),
        })
    }

    pub(crate) const fn addr(&self) -> SocketAddrV4 {
        self.addr
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    /// The URL whole, as an HTTP request names it.
    pub(crate) fn whole(&self) -> String {
        format!("http://{}{}", self.addr, self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_router_url_reads_its_address_its_port_and_its_path_and_joins_a_reference() {
        let url = RouterUrl::parse("HTTP://192.168.0.1:1900/igd/desc.xml?v=1#top").unwrap();
        assert_eq!(
            url.addr(),
            SocketAddrV4::new(Ipv4Addr::new(192, 168, 0, 1), 1900)
        );
        assert_eq!(url.path(), "/igd/desc.xml?v=1");
        assert_eq!(url.whole(), "http://192.168.0.1:1900/igd/desc.xml?v=1");
        // Port 80 when it names none, and the root when it names no path.
        let bare = RouterUrl::parse("http://10.0.0.1").unwrap();
        assert_eq!((bare.addr().port(), bare.path()), (80, "/"));
        // A host name, IPv6, another scheme and port 0 are no router's URL.
        for text in [
            "http://router.lan/desc.xml",
            "http://[fe80::1]:1900/desc.xml",
            "https://192.168.0.1/desc.xml",
            "http://192.168.0.1:0/desc.xml",
        ] {
            assert_eq!(RouterUrl::parse(text), None, "{text}");
        }
        // A path from the root, a path beside the description's, and a whole URL.
        let path = |reference: &str| url.join(reference).map(|joined| joined.whole());
        assert_eq!(path("/ctl").as_deref(), Some("http://192.168.0.1:1900/ctl"));
        assert_eq!(
            path("ctl/ip").as_deref(),
            Some("http://192.168.0.1:1900/igd/ctl/ip")
        );
        assert_eq!(
            path("http://192.168.0.1:5000/ctl").as_deref(),
            Some("http://192.168.0.1:5000/ctl")
        );
        assert_eq!(path(""), None);
        assert_eq!(path("//192.168.0.2/ctl"), None);
        assert_eq!(path("ftp://192.168.0.1/ctl"), None);
    }
}
