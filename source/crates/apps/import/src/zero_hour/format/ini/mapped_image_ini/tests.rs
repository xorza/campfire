use super::*;

#[test]
fn an_image_is_its_texture_and_its_rectangle_the_last_read_of_its_name() {
    let mut images = MappedImageIni::default();
    let first = "MappedImage SACDozer\r
  Texture = SAUserInterface512_005.tga\r
  TextureWidth = 512\r
  TextureHeight = 512\r
  Coords = Left:63 Top:301 Right:123 Bottom:349\r
  Status = NONE\r
End\r
MappedImage SSSell2\r
  Texture = SSUserInterface512_002.tga\r
  Coords = Left:187 Top:1 Right:247 Bottom:49\r
End\r
MappedImage Turned\r
  Texture = Other.tga\r
  Coords = Left:0 Top:0 Right:48 Bottom:60\r
  Status = ROTATED_90_CLOCKWISE\r
End\r
";
    images.read(first.as_bytes()).unwrap();
    // Of any case.
    assert_eq!(
        images.get("sacdozer"),
        Some(&MappedImage {
            texture: "SAUserInterface512_005.tga".to_owned(),
            coords: [63, 301, 123, 349],
            rotated: false,
        })
    );
    assert!(images.get("TURNED").unwrap().rotated);
    // A later file's image of a name replaces the earlier whole.
    images
        .read(b"MappedImage SSSell2\r\n  Coords = Left:1 Top:2 Right:3 Bottom:4\r\nEnd\r\n")
        .unwrap();
    assert_eq!(
        images.get("SSSell2"),
        Some(&MappedImage {
            texture: String::new(),
            coords: [1, 2, 3, 4],
            rotated: false,
        })
    );
    assert_eq!(images.get("Missing"), None);
}
