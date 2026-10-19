/// A name that is neither a DNS name nor an IP address, which no certificate can be checked for.
#[derive(Debug, thiserror::Error)]
#[error("{name:?} is no DNS name or IP address")]
pub struct InvalidServerName {
    pub name: String,
}
