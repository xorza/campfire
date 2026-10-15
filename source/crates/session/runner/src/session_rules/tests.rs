use campfire_common::Ticks;
use campfire_package_disk::internals::TestMode;
use campfire_protocol::SeedChain;
use campfire_visibility::VisibilityDecodeError;

use super::*;

type Change = fn(&mut SessionTerms);

#[test]
fn the_rules_build_terms_of_the_packages_and_refuse_terms_of_others() {
    // The 3v3 runs from 20 to 60 Hz.
    let packages = TestMode::Moba3v3.packages();
    let rules = SessionRules::of(&packages);
    let key = XOnlyPublicKey::from_byte_array(&[8; 32]).unwrap();
    let commitment = SeedChain::new([7; 32], NonZeroU32::MIN).commitment();
    let inputs = InputRules {
        max_input_delay: Ticks::new(3),
        max_input_lead: Ticks::new(5),
        max_payload_len: 7,
        max_inputs_per_tick: 2,
    };
    let plan = [SlotPlan::Player, SlotPlan::Bot, SlotPlan::Open];
    let terms = |hz| {
        let hz = NonZeroU32::new(hz).unwrap();
        let visibility = rules.visibility().settings();
        rules.terms(key, commitment, hz, inputs, plan, visibility)
    };
    for hz in [19, 61] {
        assert_eq!(
            terms(hz),
            Err(TermsError::TickRate(NonZeroU32::new(hz).unwrap()))
        );
    }
    assert!(terms(20).is_ok());
    let mut fixed = terms(60).unwrap();
    let dependencies: Vec<_> = packages.dependency_fingerprints().collect();
    assert_eq!(
        fixed,
        SessionTerms {
            server_key: key,
            tick_hz: NonZeroU32::new(60).unwrap(),
            max_input_delay: Ticks::new(3),
            max_input_lead: Ticks::new(5),
            max_payload_len: 7,
            max_inputs_per_tick: 2,
            seed_commitment: commitment,
            release: EngineRelease::CURRENT,
            mode: packages.fingerprint(),
            map: TestMode::Moba3v3.map(),
            dependencies,
            slots: plan.to_vec(),
            // The 3v3 sets no visibility: the defaults, which admit no spectator.
            visibility: Visibility::DEFAULT.encode(),
        }
    );
    assert!(fixed.dependencies.len() > 1, "the swap below reorders them");

    // The 3v3's teams have 6 slots: a session plays 1 to 6 of them.
    let slots = |slots: u64| TermsError::Slots { slots, most: 6 };
    let changes: [(Change, TermsError); 9] = [
        (|terms| terms.slots.clear(), slots(0)),
        (|terms| terms.slots = vec![SlotPlan::Open; 7], slots(7)),
        (
            |terms| terms.release = EngineRelease::new(0, 0, 9),
            TermsError::OtherRelease(EngineRelease::new(0, 0, 9)),
        ),
        (
            |terms| terms.mode = Fingerprint::new([0; 32]),
            TermsError::OtherMode,
        ),
        (
            |terms| terms.map = MapName::new("other").unwrap(),
            TermsError::OtherMap,
        ),
        (
            |terms| terms.dependencies.swap(0, 1),
            TermsError::OtherDependencies,
        ),
        (
            |terms| terms.dependencies.truncate(1),
            TermsError::OtherDependencies,
        ),
        (
            |terms| terms.visibility.truncate(19),
            TermsError::Visibility(VisibilityDecodeError::Length(19)),
        ),
        // A player's resources to its allies, which the mode keeps to the owner, as it leaves
        // its host only the spectators' fog and what a player out of play sees: resources'
        // byte, the eighth.
        (|terms| terms.visibility[7] = 1, TermsError::OtherVisibility),
    ];
    for (change, error) in changes {
        let mut other = fixed.clone();
        change(&mut other);
        assert_eq!(rules.check(&other), Err(error));
    }
    // What the mode leaves its host, and the spectators the host admits, any terms may set:
    // after the 14 kinds' bytes, the fog to `teams`, what a player out of play sees to
    // `spectators`, and 3 spectators.
    fixed.visibility[14..].copy_from_slice(&[1, 1, 3, 0, 0, 0]);
    let visibility = rules.check(&fixed).unwrap();
    assert_eq!(visibility.max_spectators(), 3);
}
