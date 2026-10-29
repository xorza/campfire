use campfire_codec::{Binary, BinaryError};

use super::*;

#[test]
fn a_clip_of_three_empties_at_its_third_shot_and_a_full_clip_is_not_held() {
    let clip = ClipRule {
        shots: NonZeroU16::new(3).unwrap(),
        reload: Ticks::new(21),
        refill: ClipRefill::Time,
    };
    let (rifle, pistol) = (ActionId::new(4), ActionId::new(2));
    let mut shots = ClipShots::default();
    // The rifle's first two shots leave it 1 and 2 fired; its third empties it, and its
    // clip, full again, leaves the list as it began.
    assert!(!shots.shoot(rifle, clip));
    assert!(!shots.shoot(rifle, clip));
    assert_eq!(shots.fired(rifle), 2);
    // A shot of another weapon between them counts on its own, kept in the order of ids.
    assert!(!shots.shoot(pistol, clip));
    assert_eq!(
        shots.0.iter().map(|f| f.action).collect::<Vec<_>>(),
        [pistol, rifle]
    );
    assert!(shots.shoot(rifle, clip));
    assert_eq!(shots.fired(rifle), 0);
    assert_eq!(
        shots.0,
        [Fired {
            action: pistol,
            shots: 1
        }]
    );
    // A clip of one empties at every shot, and is never held.
    let one = ClipRule {
        shots: NonZeroU16::MIN,
        reload: Ticks::new(21),
        refill: ClipRefill::Time,
    };
    let mut single = ClipShots::default();
    assert!(single.shoot(rifle, one));
    assert_eq!(single, ClipShots::default());
    // A clip of two that refills at home stays empty after its second shot, its two shots kept,
    // and its home refills it: to one shot fired, then full, which leaves the list. Its reload
    // gives one shot back in ⌈21 × 1 ÷ 2⌉ = 11 ticks, two in 21.
    let home = ClipRule {
        shots: NonZeroU16::new(2).unwrap(),
        reload: Ticks::new(21),
        refill: ClipRefill::Home,
    };
    let mut jet = ClipShots::default();
    assert!(!jet.shoot(rifle, home));
    assert!(!jet.is_empty(rifle, home.shots));
    assert!(!jet.shoot(rifle, home));
    assert!(jet.is_empty(rifle, home.shots));
    assert_eq!(jet.fired(rifle), 2);
    let at = |fired| HomeClip {
        action: rifle,
        clip: home,
        fired,
    };
    assert_eq!(
        [at(1), at(2)].map(|home| home.reload_ticks()),
        [Ticks::new(11), Ticks::new(21)]
    );
    jet.set(rifle, 1);
    assert_eq!(
        (jet.fired(rifle), jet.is_empty(rifle, home.shots)),
        (1, false)
    );
    jet.set(rifle, 0);
    assert_eq!(jet, ClipShots::default());
    // A snapshot out of order, or of a weapon of no shot, does not decode.
    let encode = |fired: &[(u32, u16)]| {
        let fired: Vec<Fired> = fired
            .iter()
            .map(|&(action, shots)| Fired {
                action: ActionId::new(action),
                shots,
            })
            .collect();
        Binary::encode(&fired)
    };
    assert!(Binary::decode::<ClipShots>(&encode(&[(2, 1), (4, 2)])).is_ok());
    let decode = |shots: &[(u32, u16)]| Binary::decode::<ClipShots>(&encode(shots)).err();
    assert_eq!(decode(&[(4, 2), (2, 1)]), Some(BinaryError::REFUSED));
    assert_eq!(decode(&[(2, 1), (2, 1)]), Some(BinaryError::REFUSED));
    assert_eq!(decode(&[(2, 0)]), Some(BinaryError::REFUSED));
}
