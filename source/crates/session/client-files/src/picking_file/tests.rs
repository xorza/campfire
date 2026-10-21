use campfire_codec::Toml;

use super::*;

#[test]
fn a_picking_file_gives_its_tiers_and_least_radius_and_refuses_a_negative_radius() {
    let filter = |text| FilterData::parse(text).unwrap();
    let file = PickingFile {
        tiers: vec![filter("enemies:avatar"), filter("enemies")],
        least_radius_px: PixelRadius::new(10.0).unwrap(),
    };
    let text = "tiers = [\"enemies:avatar\", \"enemies\"]\nleast_radius_px = 10\n";
    assert_eq!(Toml::parse::<PickingFile>(text).unwrap(), file);
    assert_eq!(
        Toml::parse::<PickingFile>(&Toml::write(&file)).unwrap(),
        file
    );
    // No file's fields: one tier, of no filter, and a least radius of 0.
    let empty = Toml::parse::<PickingFile>("").unwrap();
    assert_eq!((empty.tiers, empty.least_radius_px.get()), (vec![], 0.0));
    let refusal = |text: &str| {
        let refused = Toml::parse::<PickingFile>(text).unwrap_err();
        refused.message().to_owned()
    };
    assert_eq!(
        refusal("least_radius_px = -1"),
        "a radius is finite and at least 0"
    );
    assert_eq!(refusal("tiers = [\"friends\"]"), "filter \"friends\"");
    assert_eq!(
        refusal("radius = 1"),
        "unknown field `radius`, expected `tiers` or `least_radius_px`"
    );
}
