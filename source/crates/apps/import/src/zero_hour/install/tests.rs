use campfire_store::internals::TestDir;

use super::*;
use crate::zero_hour::format::big::big_archive::samples::big;
use crate::zero_hour::format::big::error::ArchiveError;
use crate::zero_hour::install::samples::FixtureInstall;

#[test]
fn an_install_reads_each_file_from_the_first_archive_that_holds_it() {
    let FixtureInstall { test_dir, install } = FixtureInstall::new();
    // The order, ignoring case: `a.big`, `b.big`, `c.big`, `zh_generals\base.big`; the
    // duplicate skipped, the text file no archive.
    let names: Vec<String> = install
        .archives
        .iter()
        .map(|archive| archive.name.clone())
        .collect();
    assert_eq!(names, ["A.big", "b.big", "c.BIG", "ZH_Generals/base.big"]);
    assert_eq!(
        install.read(&ArchivePath::of("shared.ini")).unwrap(),
        b"from A"
    );
    assert_eq!(
        install.read(&ArchivePath::of("SHARED.INI")).unwrap(),
        b"from A"
    );
    assert_eq!(install.read(&ArchivePath::of("data/A.ini")).unwrap(), b"A1");
    assert_eq!(install.read(&ArchivePath::of("b\\B.INI")).unwrap(), b"b");
    assert_eq!(install.read(&ArchivePath::of("c.ini")).unwrap(), b"c");
    assert_eq!(
        install.read(&ArchivePath::of("base.ini")).unwrap(),
        b"base only"
    );
    let texture = |path: &str, kind| InstallTexture {
        path: ArchivePath::of(path),
        kind,
    };
    assert_eq!(
        install.textures(),
        [
            texture("art\\terrain\\tsand.tga", TextureKind::Tga),
            texture("art\\textures\\rock.dds", TextureKind::Dds),
            texture("art\\textures\\sign.tga", TextureKind::Tga),
        ]
    );
    // The language directory whose `Art` the archives hold; the object INI files in the game's
    // order: the default file, then the file, its directory's files, and its directories'
    // below.
    assert_eq!(install.language().unwrap().as_deref(), Some("english"));
    assert_eq!(
        install.inis("Object"),
        [
            "data\\ini\\default\\object.ini",
            "data\\ini\\object.ini",
            "data\\ini\\object\\misc.ini",
            "data\\ini\\object\\deep\\more.ini",
        ]
        .map(ArchivePath::of)
    );
    // The map in its own directory, by its key; not the one in another's.
    assert_eq!(
        install.maps(),
        [InstallMap {
            path: ArchivePath::of("maps\\fixture map\\fixture map.map"),
            dir: "fixture map".to_owned(),
        }]
    );
    assert!(matches!(
        install.read(&ArchivePath::of("missing.ini")),
        Err(ZeroHourError::NotInArchives(path)) if path == "missing.ini"
    ));
    // Each archive's hash, in the load order.
    let hashed = install.hashed().unwrap();
    let expected: Vec<HashedArchive> = [
        "zh/A.big",
        "zh/b.big",
        "zh/c.BIG",
        "zh/ZH_Generals/base.big",
    ]
    .into_iter()
    .zip(&names)
    .map(|(file, name)| HashedArchive {
        name: name.clone(),
        sha256: ContentHash::of(&test_dir.read(file)),
    })
    .collect();
    assert_eq!(hashed, expected);
    // The install holds no archive open, so another program may replace one; a read of it
    // then is refused, as its length is another, and a read of another archive is not. A
    // second language directory with `Art` leaves the importer unable to tell which one the
    // game reads.
    test_dir.write("zh/c.BIG", big(&[("Data\\German\\Art\\W3D\\x.w3d", b"x")]));
    let changed = test_dir.path("zh/c.BIG");
    assert!(matches!(
        install.read(&ArchivePath::of("c.ini")),
        Err(ZeroHourError::ArchiveChanged { archive }) if archive == changed
    ));
    assert!(matches!(
        install.hashed(),
        Err(ZeroHourError::ArchiveChanged { .. })
    ));
    assert_eq!(install.read(&ArchivePath::of("data/A.ini")).unwrap(), b"A1");
    assert!(matches!(
        Install::open(&test_dir.path("zh")).unwrap().language(),
        Err(ZeroHourError::Languages(languages)) if languages == ["english", "german"]
    ));
}

#[test]
fn an_archive_the_game_cannot_read_fails_the_install() {
    let full = big(&[("a.ini", b"abc")]);
    // Its tag, a header past the file, and an entry past the file.
    let mut wrong = full.clone();
    wrong[3] = b'G';
    let mut long_header = full.clone();
    long_header[15] = 200;
    let cut = full[..full.len() - 1].to_vec();
    for (bytes, expected) in [
        (wrong, ArchiveError::NotBig),
        (long_header, ArchiveError::Short),
        (cut, ArchiveError::EntryPastEnd("a.ini".to_owned())),
        (b"BIG".to_vec(), ArchiveError::Short),
    ] {
        let test_dir = TestDir::new();
        test_dir.write("zh/x.big", bytes);
        let error = Install::open(&test_dir.path("zh")).unwrap_err();
        assert!(
            matches!(
                &error,
                ZeroHourError::Archive { archive, error }
                    if *archive == test_dir.path("zh/x.big") && *error == expected
            ),
            "{error:?}"
        );
    }
    // Two archives of one path to the game, its case aside, as a changed `INIZH.BIG` beside
    // the genuine `INIZH.big`: the game reads one, so the import reads neither. A file
    // system that folds case, as Windows' and macOS's do, holds one file of the two names,
    // which is no clash.
    let test_dir = TestDir::new();
    test_dir.write("zh/INIZH.big", &full);
    test_dir.write("zh/INIZH.BIG", big(&[("a.ini", b"xyz")]));
    let opened = Install::open(&test_dir.path("zh"));
    if test_dir.names("zh").len() == 2 {
        assert!(matches!(
            opened,
            Err(ZeroHourError::ArchiveClash { first, second })
                if first == "INIZH.BIG" && second == "INIZH.big"
        ));
    } else {
        assert!(opened.is_ok());
    }
}
