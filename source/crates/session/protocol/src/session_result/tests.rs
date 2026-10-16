use super::*;
use crate::harness::test_key::TestKey;
use crate::session_id::SessionId;
use crate::verifier::Verifier;

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
    let signature = TestKey::signer(server).sign(&InSession {
        session_id: id,
        record: &result,
    });
    let signed_by = |record: &SessionResult, session_id| {
        Verifier::new().verifies(&signature, &key, &InSession { session_id, record })
    };
    assert!(signed_by(&result, id));
    assert!(!signed_by(&result, SessionId::new([9; 32])));
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
        assert!(!signed_by(&other, id), "{other:?}");
    }
}
