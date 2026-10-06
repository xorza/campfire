use super::*;
use crate::test_key::TestKey;

#[test]
fn a_results_signature_holds_only_over_it() {
    let secp = Secp256k1::new();
    let server = TestKey::server();
    let key = server.x_only_public_key().0;
    let id = SessionId::new([8; 32]);
    let result = SessionResult {
        tick: Tick::new(7),
        outcome: Outcome::Won { team: 1 },
        state_hash: StateHash::new([3; 32]),
    };
    let signature = result.sign(&secp, &server, id, &[0; 32]);
    assert!(result.signed_by(&secp, &key, id, &signature));
    assert!(!result.signed_by(&secp, &key, SessionId::new([9; 32]), &signature));
    let others = [
        SessionResult {
            tick: Tick::new(8),
            ..result
        },
        SessionResult {
            outcome: Outcome::Won { team: 0 },
            ..result
        },
        SessionResult {
            outcome: Outcome::Aborted,
            ..result
        },
        SessionResult {
            state_hash: StateHash::new([4; 32]),
            ..result
        },
    ];
    for other in others {
        assert!(!other.signed_by(&secp, &key, id, &signature), "{other:?}");
    }
}
