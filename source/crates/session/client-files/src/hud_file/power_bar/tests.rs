use campfire_codec::Toml;

use super::*;

#[test]
fn a_bar_reads_its_fields_and_refuses_a_base_below_2_or_intervals_of_none() {
    let bar = |base: u8, intervals: &str| {
        let icon = |at: u16| {
            format!(
                "{{ texture = \"bar.ktx2\", rect = [{at}, 0, {}, 6] }}",
                at + 6
            )
        };
        let text = format!(
            "base = {base}\nintervals = \"{intervals}\"\nmargin = 5\nneedle = {}\n\
             [fills]\nenough = {}\nnear = {}\nshort = {}\n",
            icon(0),
            icon(6),
            icon(12),
            icon(18)
        );
        Toml::parse::<PowerBar>(&text)
    };
    let read = bar(7, "3").unwrap();
    assert_eq!(
        (read.base(), read.intervals(), read.margin()),
        (7, Num::int(3), 5)
    );
    assert_eq!(read.needle().rect.edges(), [0, 0, 6, 6]);
    assert_eq!(read.fills().short.rect.edges(), [18, 0, 24, 6]);
    assert!(bar(1, "3").is_err());
    assert!(bar(7, "0").is_err());
}
