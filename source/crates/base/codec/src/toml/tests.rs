use std::collections::BTreeMap;
use std::panic;

use super::*;

#[test]
fn a_table_writes_and_parses_back_and_bad_text_fails() {
    let table = BTreeMap::from([("cell", 10), ("step", 5)]);
    let text = Toml::write(&table);
    assert_eq!(text, "cell = 10\nstep = 5\n");
    assert_eq!(
        Toml::parse::<BTreeMap<String, i64>>(&text).unwrap()["step"],
        5
    );
    assert!(Toml::parse::<BTreeMap<String, i64>>("cell = ").is_err());
    assert!(panic::catch_unwind(|| Toml::write(&5)).is_err());
}
