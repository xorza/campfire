use campfire_store::DurableFile;
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
    // A texture's format by its extension, of any case as a key folds it; another file is none.
    let kind = |path: &str| TextureKind::of(&ArchivePath::of(path));
    assert_eq!(kind("art\\textures\\Rock.DDS"), Some(TextureKind::Dds));
    assert_eq!(kind("art\\textures\\sign.tga"), Some(TextureKind::Tga));
    assert_eq!(kind("data\\ini\\object.ini"), None);
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
        Err(ZeroHourError::NotInArchives(path)) if path.as_str() == "missing.ini"
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
    // The install holds each archive open: another program's file of its name takes the name
    // where a rename replaces an open file, and is refused where it does not, as on Windows;
    // either way a read and the hash are still of the bytes it opened.
    let german = big(&[("Data\\German\\Art\\W3D\\x.w3d", b"x")]);
    DurableFile::write(&test_dir.path("zh/c.BIG"), &german).ok();
    assert_eq!(install.read(&ArchivePath::of("c.ini")).unwrap(), b"c");
    assert_eq!(install.hashed().unwrap(), expected);
    // A write into an archive it holds open changes it: a read of it is then refused, and a read
    // of another archive is not.
    let changed = test_dir.path("zh/b.big");
    test_dir.append("zh/b.big", b"more");
    assert!(matches!(
        install.read(&ArchivePath::of("b\\B.INI")),
        Err(ZeroHourError::ArchiveChanged { archive }) if archive == changed
    ));
    assert!(matches!(
        install.hashed(),
        Err(ZeroHourError::ArchiveChanged { .. })
    ));
    assert_eq!(install.read(&ArchivePath::of("data/A.ini")).unwrap(), b"A1");
    // A second language directory with `Art` leaves the importer unable to tell which one the
    // game reads; the install is closed first, as Windows replaces no file a program holds open.
    drop(install);
    test_dir.write("zh/c.BIG", german);
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
    // A root under a link keeps the path it is given; a root that is a link opens by its
    // target, every link of the target's path followed, as a temporary directory's is on macOS.
    let test_dir = TestDir::new();
    test_dir.write("real/zh/x.big", b"BIG");
    test_dir.link_dir("real", "up");
    test_dir.link_dir("real/zh", "zh");
    let archive_of = |root: &str| match Install::open(&test_dir.path(root)) {
        Err(ZeroHourError::Archive { archive, .. }) => archive,
        other => panic!("{other:?}"),
    };
    assert_eq!(archive_of("up/zh"), test_dir.path("up/zh/x.big"));
    let target = DirEntries::link_target(&test_dir.path("real/zh")).unwrap();
    assert_eq!(archive_of("zh"), target.join("x.big"));
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
