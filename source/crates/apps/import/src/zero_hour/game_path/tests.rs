use super::*;

#[test]
fn a_name_makes_a_package_path_or_an_error_and_a_link_climbs_to_the_common_directory() {
    assert_eq!(
        package_path("client/models/avtank.glb".to_owned())
            .unwrap()
            .as_str(),
        "client/models/avtank.glb"
    );
    for refused in [
        "client/models/a:b.glb",
        "client/../x.glb",
        "client/models/aux.glb",
    ] {
        assert_eq!(
            package_path(refused.to_owned()),
            Err(NoPackagePath(refused.to_owned()))
        );
    }
    let path = |text: &str| PackagePath::parse(text).unwrap();
    let texture = path("client/textures/art/a b.ktx2");
    // From a model one directory under `client/`, one `..`; from one two under, two; from
    // the texture's own directory, none; a space escaped.
    assert_eq!(
        relative_uri(&path("client/models/m.glb"), &texture),
        "../textures/art/a%20b.ktx2"
    );
    assert_eq!(
        relative_uri(&path("client/models/deep/m.glb"), &texture),
        "../../textures/art/a%20b.ktx2"
    );
    assert_eq!(
        relative_uri(&path("client/textures/art/m.glb"), &texture),
        "a%20b.ktx2"
    );
}
