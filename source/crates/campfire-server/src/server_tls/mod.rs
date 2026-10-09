use campfire_protocol::CertificateHash;
use campfire_store::{PathError, ReadError, SecretFile};
use wtransport::Identity;
use wtransport::tls::{Certificate, CertificateChain, PrivateKey};
use zeroize::Zeroizing;

use crate::server_tls::error::TlsError;

mod error;

/// How long a new certificate holds, in seconds: the 14 days WebTransport allows a self-signed
/// one at most.
const LIFETIME: u64 = 14 * 86_400;
/// A certificate that expires within this many seconds is made again at a start with no session
/// to restore, so a session and its restores run inside one certificate's life.
const RENEW_WITHIN: u64 = 2 * 86_400;
const EXPIRES_BYTES: usize = size_of::<u64>();
const LEN_BYTES: usize = size_of::<u32>();
/// The most bytes the file may hold: far past a self-signed certificate and its key, the 2.5 KB
/// of an RSA 4096 one among them, so a file of another kind is refused before it is read whole.
const MAX_LEN: usize = 16 * 1024;

/// The server's TLS identity, which its data directory keeps in `tls`, so a restored server
/// presents the certificate its clients pinned: a self-signed certificate for `localhost`, its
/// key, and when it expires, in Unix seconds. The file is a secret file, written whole and
/// atomically: `u64` expiry, `u32` certificate length, the certificate's DER, then the key's
/// PKCS#8 DER, all little-endian; its bytes, here, are zeroed when dropped.
#[derive(Debug)]
pub(crate) struct ServerTls {
    identity: Identity,
    expires: u64,
}

impl ServerTls {
    /// The identity `file` keeps, at Unix second `now`; a new one, written, when it keeps none,
    /// or when its certificate expires within `RENEW_WITHIN` and no session restores, as
    /// `restoring` says. The data directory's lock is held, so the file is replaced, never
    /// raced. An error when the file does not read or is not written.
    pub(crate) fn open(
        file: &SecretFile,
        now: u64,
        restoring: bool,
    ) -> Result<ServerTls, TlsError> {
        match file.read(MAX_LEN) {
            Ok(bytes) => {
                let kept = ServerTls::decode(&bytes)?;
                if restoring || kept.expires.saturating_sub(now) > RENEW_WITHIN {
                    return Ok(kept);
                }
            }
            Err(PathError {
                error: ReadError::Missing,
                ..
            }) => {}
            Err(error) => return Err(TlsError::Read(error)),
        }
        let made = ServerTls {
            identity: Identity::self_signed(["localhost"]).expect("a fixed name is a valid SAN"),
            expires: now + LIFETIME,
        };
        file.replace(&made.encode()).map_err(TlsError::Write)?;
        Ok(made)
    }

    /// The hash of the certificate, which clients pin.
    pub(crate) fn certificate(&self) -> CertificateHash {
        let chain = self.identity.certificate_chain();
        CertificateHash::new(*chain.as_slice()[0].hash().as_ref())
    }

    pub(crate) fn into_identity(self) -> Identity {
        self.identity
    }

    /// The file's bytes, in a buffer sized whole, so no growth leaves a copy of the key behind.
    fn encode(&self) -> Zeroizing<Vec<u8>> {
        let certificate = self.identity.certificate_chain().as_slice()[0].der();
        let key = self.identity.private_key().secret_der();
        let len = u32::try_from(certificate.len()).expect("a certificate fits u32 bytes");
        let mut bytes = Zeroizing::new(Vec::with_capacity(
            EXPIRES_BYTES + LEN_BYTES + certificate.len() + key.len(),
        ));
        bytes.extend_from_slice(&self.expires.to_le_bytes());
        bytes.extend_from_slice(&len.to_le_bytes());
        bytes.extend_from_slice(certificate);
        bytes.extend_from_slice(key);
        bytes
    }

    fn decode(bytes: &[u8]) -> Result<ServerTls, TlsError> {
        let (expires, rest) = bytes
            .split_first_chunk::<EXPIRES_BYTES>()
            .ok_or(TlsError::Truncated)?;
        let (len, rest) = rest
            .split_first_chunk::<LEN_BYTES>()
            .ok_or(TlsError::Truncated)?;
        let len = usize::try_from(u32::from_le_bytes(*len)).expect("u32 fits usize");
        let (certificate, key) = rest.split_at_checked(len).ok_or(TlsError::Truncated)?;
        if key.is_empty() {
            return Err(TlsError::Truncated);
        }
        let certificate =
            Certificate::from_der(certificate.to_vec()).map_err(TlsError::Certificate)?;
        let key = PrivateKey::from_der_pkcs8(key.to_vec());
        Ok(ServerTls {
            identity: Identity::new(CertificateChain::new(vec![certificate]), key),
            expires: u64::from_le_bytes(*expires),
        })
    }
}

#[cfg(test)]
mod tests {
    use campfire_store::Scratch;

    use super::*;

    const NOW: u64 = 1_700_000_000;

    #[test]
    fn two_starts_keep_one_certificate_until_it_nears_its_end() {
        let scratch = Scratch::new();
        let file = SecretFile::at(scratch.path("tls"));
        let open = |now, restoring| ServerTls::open(&file, now, restoring).unwrap();

        // A first start makes the certificate, 14 days long; a second start a day later keeps it.
        let first = open(NOW, false);
        assert_eq!(first.expires, NOW + 14 * 86_400);
        let hash = first.certificate();
        assert_eq!(open(NOW + 86_400, false).certificate(), hash);
        // 2 days and a second before its end it is kept; 2 days before, a start that restores
        // a session keeps it, and one that does not makes a new one, which lasts 14 days more.
        let renewal = NOW + 12 * 86_400;
        assert_eq!(open(renewal - 1, false).certificate(), hash);
        assert_eq!(open(renewal, true).certificate(), hash);
        let renewed = open(renewal, false);
        assert_ne!(renewed.certificate(), hash);
        assert_eq!(renewed.expires, renewal + 14 * 86_400);
        assert_eq!(open(renewal, false).certificate(), renewed.certificate());

        // A file cut short, or whose certificate does not parse, is refused.
        let whole = scratch.read("tls");
        for cut in [0, EXPIRES_BYTES + 2, EXPIRES_BYTES + LEN_BYTES + 10] {
            scratch.write("tls", &whole[..cut]);
            assert!(matches!(
                ServerTls::open(&file, NOW, true),
                Err(TlsError::Truncated)
            ));
        }
        let mut flawed = whole.clone();
        flawed[EXPIRES_BYTES + LEN_BYTES] ^= 0xFF;
        scratch.write("tls", &flawed);
        assert!(matches!(
            ServerTls::open(&file, NOW, true),
            Err(TlsError::Certificate(_))
        ));
        // One longer than a TLS file may be, and one others may read, are refused; the identity
        // they would hold is not made again over them.
        scratch.write("tls", vec![0; MAX_LEN + 1]);
        assert!(matches!(
            ServerTls::open(&file, NOW, false),
            Err(TlsError::Read(PathError {
                error: ReadError::TooLarge { max: MAX_LEN },
                ..
            }))
        ));
        scratch.write("tls", &whole);
        assert_eq!(open(NOW, true).certificate(), renewed.certificate());
        file.expose();
        assert!(matches!(
            ServerTls::open(&file, NOW, false),
            Err(TlsError::Read(PathError {
                error: ReadError::Exposed(_),
                ..
            }))
        ));
        assert_eq!(scratch.read("tls"), whole);
    }
}
