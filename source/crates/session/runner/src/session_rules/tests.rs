use campfire_common::{MapName, PackageFingerprint, Ticks};
use campfire_mode::{PickError, SeatPickError};
use campfire_protocol::SeedChain;
use campfire_test_packages::TestMode;
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
        max_delay: Ticks::new(3),
        max_lead: Ticks::new(5),
        max_payload_len: 7,
        max_per_tick: 2,
    };
    let plan = [SeatPlan::Player, SeatPlan::Bot, SeatPlan::Open];
    let terms = |hz| {
        let hz = NonZeroU32::new(hz).unwrap();
        let visibility = rules.visibility().settings();
        rules.terms(HostChoices {
            server_key: key,
            seed_commitment: commitment,
            tick_hz: hz,
            inputs,
            seats: plan.to_vec(),
            visibility,
            picks: &HostPicks::defaults(packages.data(), plan.len()),
        })
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
            inputs,
            seed_commitment: commitment,
            release: Version::ENGINE,
            mode: packages.fingerprint(),
            map: TestMode::Moba3v3.map(),
            dependencies,
            seats: plan.to_vec(),
            // The 3v3 sets no visibility: the defaults, which admit no spectator.
            visibility: Visibility::DEFAULT.encode(),
            // The 3v3 declares no option, and no choice of values.
            options: Vec::new(),
            seat_picks: Vec::new(),
        }
    );
    assert!(fixed.dependencies.len() > 1, "the swap below reorders them");

    // The 3v3's teams have 6 seats: a session plays 1 to 6 of them.
    let seats = |seats: u64| TermsError::Seats { seats, most: 6 };
    let changes: [(Change, TermsError); 11] = [
        (|terms| terms.seats.clear(), seats(0)),
        (|terms| terms.seats = vec![SeatPlan::Open; 7], seats(7)),
        (
            |terms| terms.release = Version::new(0, 0, 9),
            TermsError::OtherRelease(Version::new(0, 0, 9)),
        ),
        (
            |terms| terms.mode = PackageFingerprint::new([0; 32]),
            TermsError::OtherMode,
        ),
        (
            |terms| terms.map = MapName::parse("other").unwrap(),
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
        // A pick of an option the mode does not declare.
        (
            |terms| terms.options.push(0),
            TermsError::Picks(HostPickError::Option(PickError::Count {
                given: 1,
                options: 0,
            })),
        ),
        // A seat's pick of a choice of values the mode does not declare.
        (
            |terms| terms.seat_picks.push(0),
            TermsError::Picks(HostPickError::Seat(SeatPickError::Count {
                given: 1,
                expected: 0,
            })),
        ),
    ];
    for (change, error) in changes {
        let mut other = fixed.clone();
        change(&mut other);
        assert_eq!(rules.check(&other), Err(error));
    }
    // What the mode leaves its host, and the spectators the host admits, any terms may set:
    // after the 17 kinds' bytes, the fog to `teams`, what a player out of play sees to
    // `spectators`, and 3 spectators.
    fixed.visibility[17..].copy_from_slice(&[1, 1, 3, 0, 0, 0]);
    let checked = rules.check(&fixed).unwrap();
    assert_eq!(checked.visibility.max_spectators(), 3);
    assert!(checked.picks.options.places().is_empty());
    assert!(checked.picks.seats.places().is_empty());
}
