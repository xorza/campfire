use super::*;

/// A raw key's `access_denied` and a certificate's `unknown_ca` are a refused identity;
/// `illegal_parameter` and `no_application_protocol` (120) are the transport's errors.
#[test]
fn a_refused_identity_is_a_wrong_tls_key_and_any_other_alert_its_code() {
    assert_eq!(unlink_reason(49), UnlinkReason::WrongTlsKey);
    assert_eq!(unlink_reason(48), UnlinkReason::WrongTlsKey);
    assert_eq!(
        unlink_reason(47),
        UnlinkReason::Transport { code: 0x100 + 47 }
    );
    assert_eq!(
        unlink_reason(120),
        UnlinkReason::Transport { code: 0x100 + 120 }
    );
}
