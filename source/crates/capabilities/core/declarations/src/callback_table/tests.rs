use crate::callback_table::CallbackTable;

#[test]
fn a_place_takes_one_callback_and_the_rest_stay_empty() {
    let mut table = CallbackTable::<u8, 3>::default();
    table.register(2, 7);
    table.register(0, 5);
    assert_eq!([0, 1, 2].map(|at| table.get(at)), [Some(5), None, Some(7)]);
    assert_eq!(table.iter().collect::<Vec<_>>(), [5, 7]);
}

#[test]
#[should_panic(expected = "a place of a registry takes one callback")]
fn a_place_refuses_a_second_callback() {
    let mut table = CallbackTable::<u8, 1>::default();
    table.register(0, 1);
    table.register(0, 2);
}
