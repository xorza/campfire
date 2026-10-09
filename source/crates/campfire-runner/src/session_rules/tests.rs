use campfire_common::Ticks;
use campfire_package::PackageDir;
use campfire_protocol::SeedChain;

use super::*;

type Change = fn(&mut SessionTerms);

#[test]
fn the_rules_build_terms_of_the_packages_and_refuse_terms_of_others() {
    // The 3v3 runs from 20 to 60 Hz.
    let dir = PackageDir::workspace("test/moba/modes/3v3");
    let packages = ModePackages::from_dir(&dir, &MapName::new("two_lanes").unwrap()).unwrap();
    let rules = SessionRules::of(&packages);
    let key = XOnlyPublicKey::from_byte_array(&[8; 32]).unwrap();
    let commitment = SeedChain::new([7; 32], NonZeroU32::MIN).commitment();
    let inputs = InputRules {
        max_input_delay: Ticks::new(3),
        max_input_lead: Ticks::new(5),
        max_payload_len: 7,
        max_inputs_per_tick: 2,
    };
    let plan = vec![SlotPlan::Player, SlotPlan::Bot, SlotPlan::Open];
    let terms = |hz| {
        let hz = NonZeroU32::new(hz).unwrap();
        rules.terms(key, commitment, hz, inputs, plan.clone())
    };
    for hz in [19, 61] {
        assert_eq!(
            terms(hz),
            Err(TermsError::TickRate(NonZeroU32::new(hz).unwrap()))
        );
    }
    assert!(terms(20).is_ok());
    let fixed = terms(60).unwrap();
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
            release: RELEASE.to_owned(),
            mode: packages.fingerprint(),
            map: MapName::new("two_lanes").unwrap(),
            dependencies,
            slots: plan.clone(),
        }
    );
    assert!(fixed.dependencies.len() > 1, "the swap below reorders them");

    // The 3v3's teams have 6 slots: a session plays 1 to 6 of them.
    let slots = |slots: u64| TermsError::Slots { slots, most: 6 };
    let changes: [(Change, TermsError); 7] = [
        (|terms| terms.slots.clear(), slots(0)),
        (|terms| terms.slots = vec![SlotPlan::Open; 7], slots(7)),
        (
            |terms| terms.release = "0.0.9".to_owned(),
            TermsError::OtherRelease("0.0.9".to_owned()),
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
    ];
    for (change, error) in changes {
        let mut other = fixed.clone();
        change(&mut other);
        assert_eq!(rules.check(&other), Err(error));
    }
}
