use std::num::NonZeroU16;

use campfire_codec::{Binary, BinaryError};
use campfire_common::Ticks;

use super::*;

#[test]
fn a_clip_of_three_empties_at_its_third_shot_and_a_full_clip_is_not_held() {
    let clip = ClipRule {
        shots: NonZeroU16::new(3).unwrap(),
        reload: Ticks::new(21),
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
    };
    let mut single = ClipShots::default();
    assert!(single.shoot(rifle, one));
    assert_eq!(single, ClipShots::default());
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
