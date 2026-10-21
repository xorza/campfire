use campfire_codec::Toml;

use super::*;

#[test]
fn objects_write_and_read_back_their_looks_and_exact_places() {
    let object = ClientObject {
        look: DeclaredName::new("ptoak01").unwrap(),
        position: [Num::int(30), Num::int(-5) / Num::int(2)],
        height: Num::int(-9) / Num::int(2),
        angle: Num::from_bits(-1_493_172_476),
    };
    let objects = ClientObjects {
        objects: vec![object],
    };
    let text = Toml::write(&objects);
    assert_eq!(Toml::parse::<ClientObjects>(&text).unwrap(), objects);
    // A place reads from integers too, as a map's does; a field the format lacks is refused.
    let read = |text: &str| Toml::parse::<ClientObjects>(text);
    let whole =
        read("[[objects]]\nlook = \"a\"\nposition = [1, \"2.5\"]\nheight = 0\nangle = 90\n");
    let whole = whole.unwrap();
    assert_eq!(
        whole.objects[0].position,
        [Num::int(1), Num::int(5) / Num::int(2)]
    );
    let scaled = "[[objects]]\nlook = \"a\"\nposition = [1, 2]\nheight = 0\nangle = 0\nscale = 1\n";
    let refused = read(scaled).unwrap_err();
    assert!(
        refused.message().starts_with("unknown field `scale`"),
        "{refused}"
    );
}
