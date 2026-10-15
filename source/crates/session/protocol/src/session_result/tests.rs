use super::*;
use crate::harness::test_key::TestKey;

#[test]
fn a_results_signature_holds_only_over_it() {
    let server = TestKey::server();
    let key = server.x_only_public_key().0;
    let id = SessionId::new([8; 32]);
    let result = SessionResult {
        tick: Tick::new(7),
        outcome: Outcome::Won { team: 1 },
        state_hash: StateHash::new([3; 32]),
    };
    let signature = result.sign(&TestKey::signer(server), id);
    assert!(result.signed_by(&mut Verifier::new(), &key, id, &signature));
    assert!(!result.signed_by(
        &mut Verifier::new(),
        &key,
        SessionId::new([9; 32]),
        &signature
    ));
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
        assert!(
            !other.signed_by(&mut Verifier::new(), &key, id, &signature),
            "{other:?}"
        );
    }
}
