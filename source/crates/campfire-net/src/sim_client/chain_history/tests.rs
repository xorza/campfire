use campfire_common::{PlayerSlot, Tick};

use super::*;

#[test]
fn a_history_gives_each_head_it_holds_and_cuts_to_one() {
    // From a chain that holds 2 inputs already: seq 2 links to its head, and the history
    // notes the heads after seq 2, 3 and 4.
    let mut chain = InputChain::new(PlayerSlot::new(1), InputHash::new([1; 32]));
    chain.extend(Tick::new(0), b"a");
    chain.extend(Tick::new(0), b"b");
    let mut history = ChainHistory::of(&chain);
    let mut heads = vec![chain.head()];
    for payload in [b"c", b"d", b"e"] {
        chain.extend(Tick::new(1), payload);
        history.push(chain.head());
        heads.push(chain.head());
    }
    assert_eq!(
        [1, 2, 3, 4, 5, 6].map(|next_seq| history.head_at(next_seq)),
        [
            None,
            Some(heads[0]),
            Some(heads[1]),
            Some(heads[2]),
            Some(heads[3]),
            None
        ]
    );
    // The log holds 3 of the 5: the two after it go.
    assert_eq!(history.cut(3), 2);
    assert_eq!(history.head_at(3), Some(heads[1]));
    assert_eq!(history.head_at(4), None);
    assert_eq!(history.cut(2), 1);
    assert_eq!(history.cut(2), 0);
    // Two inputs again, then a receipt for the first, seq 2: the history holds from 3 on.
    history.push(heads[1]);
    history.push(heads[2]);
    history.forget_before(3);
    assert_eq!(history.head_at(2), None);
    assert_eq!(history.head_at(3), Some(heads[1]));
    assert_eq!(history.head_at(4), Some(heads[2]));
    assert_eq!(history.head_at(5), None);
}
