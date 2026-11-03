use std::num::NonZeroU8;

use campfire_common::Ticks;

use super::*;

#[test]
fn a_producer_gives_its_door_times_and_one_of_no_doors_none() {
    let [opening, open, closing] = [3, 4, 5].map(Ticks::new);
    let doors = DoorTimes {
        count: 2,
        opening,
        open,
        closing,
    };
    let producer = |doors| ProducerSpec {
        queue: NonZeroU8::MIN,
        exit: None,
        rally: None,
        gathers: false,
        free: Box::new([]),
        places: Box::new([]),
        doors,
    };
    let [garage, barracks, tree] = [0, 1, 2].map(UnitType::new);
    let mut book = ById::<UnitType, ProducerSpec>::default().reopen();
    book.set(garage, producer(Some(doors)));
    book.set(barracks, producer(None));
    let found = ProducerDoors::of_producers(&book.build());
    let doors_of = [garage, barracks, tree].map(|unit_type| found.of(unit_type));
    assert_eq!(doors_of, [Some(doors), None, None]);
}
