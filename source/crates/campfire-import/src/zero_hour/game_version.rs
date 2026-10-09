use std::collections::{BTreeMap, BTreeSet};

use crate::error::ImportError;
use crate::zero_hour::error::{VersionDifference, ZeroHourError};
use crate::zero_hour::install::HashedArchive;

/// A version of Zero Hour the importer knows: its name, and the SHA-256 of each archive the game
/// reads, so the importer reads only an install whose every byte it knows the meaning of.
#[derive(Debug)]
pub struct GameVersion {
    pub name: &'static str,
    pub archives: &'static [ArchiveHash],
}

/// The first archive where an install differs from a version, by its path's ASCII lowercase, and
/// how.
#[derive(Debug)]
struct Difference {
    archive: String,
    difference: VersionDifference,
}

/// One archive of a version: its path in the install, with `/` between names, and its SHA-256.
#[derive(Debug)]
pub struct ArchiveHash {
    pub path: &'static str,
    pub sha256: [u8; 32],
}

impl ArchiveHash {
    /// The archive at `path` whose SHA-256 is the 64 lowercase hex digits `hex`.
    pub const fn new(path: &'static str, hex: &str) -> ArchiveHash {
        let hex = hex.as_bytes();
        assert!(hex.len() == 64, "a SHA-256 is 64 hex digits");
        let mut sha256 = [0; 32];
        let mut at = 0;
        while at < 32 {
            sha256[at] = digit(hex[2 * at]) << 4 | digit(hex[2 * at + 1]);
            at += 1;
        }
        ArchiveHash { path, sha256 }
    }
}

/// The value of the lowercase hex digit `byte`.
const fn digit(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => panic!("a lowercase hex digit"),
    }
}

impl GameVersion {
    /// The versions the importer reads.
    pub const KNOWN: &[GameVersion] = &[GameVersion {
        name: "Steam: Zero Hour, and Generals in ZH_Generals",
        archives: &[
            ArchiveHash::new(
                "AudioEnglishZH.big",
                "85109b5cb4a5ef75c5fdd1fe1a66957d951f98f028e10e3729dddc2309402914",
            ),
            ArchiveHash::new(
                "AudioZH.big",
                "6fbd05e43491bfd5f56c9250c3f9c30fc3061cd5867cb912c46486fc78f5e8c7",
            ),
            ArchiveHash::new(
                "EnglishZH.big",
                "d3904216ac210a363d9e0e46980c6ef70232046ba713ea137e8a87cbd57a32d6",
            ),
            ArchiveHash::new(
                "gensecZH.big",
                "ac2aaf5536a2f748dc99178aa431280f26d9d85179e6c2abedf5adfabc971b15",
            ),
            ArchiveHash::new(
                "INIZH.big",
                "1a6d41a7a2cb31e67ad2f868aca9264ad069c275e0074f8a0d970a336071e9a0",
            ),
            ArchiveHash::new(
                "MapsZH.big",
                "35cc8947f34f363d69f5045b9ac65f6ee164b8d5a382a1f7af213dbc2a744b96",
            ),
            ArchiveHash::new(
                "Music.big",
                "3f2af9c6dcc2b35852556bdc020c95e43c2f43127c50dbff018e12a2bed6116f",
            ),
            ArchiveHash::new(
                "MusicZH.big",
                "5aa7b8408e5cf61fd53633f6e6310c76d79e74d75dd3286b6e9b9c718d9c5b5e",
            ),
            ArchiveHash::new(
                "PatchData.big",
                "90952433efe55a774ed3f8375b7700b0a16c8206a760b5cdb3d8707a0e66fc0b",
            ),
            ArchiveHash::new(
                "PatchINI.big",
                "16028d315c8c4d279beed15f1836a8998ff5c9a4d0d621d4a3d37213a0d9fe62",
            ),
            ArchiveHash::new(
                "PatchWindow.big",
                "385767596cf1ff114c530ff73f83d63257c8d509dbcb8ef9f18858b1dcabbc89",
            ),
            ArchiveHash::new(
                "PatchZH.big",
                "450276fbabd19f79dc0143f70fe755e44a99b22c552bc00c5854fc810e615722",
            ),
            ArchiveHash::new(
                "ShadersZH.big",
                "6246a6906f93261669c8c19e021c8d3484a6f7449bd5e253c8e998c1a9d31d6e",
            ),
            ArchiveHash::new(
                "SpeechEnglishZH.big",
                "5b3c8b1819b1ddfeaf6045a4a5dfb9cb47ad16a508dbf17c317e11b9d310a14f",
            ),
            ArchiveHash::new(
                "SpeechZH.big",
                "4498097a3c294bc638e72e09e967bb417b0c9b171b4eee95e2c6c0bffef68fa2",
            ),
            ArchiveHash::new(
                "TerrainZH.big",
                "501ef71d12e7a1398800231f2526d47e8e583607f843a2ebcb0c622e26fc73b3",
            ),
            ArchiveHash::new(
                "TexturesZH.big",
                "aa975400abd70e45e13eacf4ab22505ea322941dfc492097c68245cd35d4b791",
            ),
            ArchiveHash::new(
                "W3DEnglishZH.big",
                "f5d164d1e294f26744e2b87ecfbe13e68affaf5ceb4cf12f43005beb8f54defd",
            ),
            ArchiveHash::new(
                "W3DZH.big",
                "e308c3aeb49d023ffd98b50400f24643a782a68ac4d2e53171c17fc45d73fdc6",
            ),
            ArchiveHash::new(
                "WindowZH.big",
                "68b519f28d012cd297fae2663d431a4b0e6e416aa7bb1a57a79e5500ebcc14b4",
            ),
            ArchiveHash::new(
                "ZH_Generals/Audio.big",
                "d522df264149e3e27fd46f027548cc38bd60614a43666555219b7b1d45fb3bad",
            ),
            ArchiveHash::new(
                "ZH_Generals/AudioEnglish.big",
                "39d67dba96111178fcceefae2bedb2dc65b55b968741162c714b333c0b0f5f2e",
            ),
            ArchiveHash::new(
                "ZH_Generals/English.big",
                "218440f15bd2f718c6897f631eeacb8a42378679bcc0a6aa8a9cf83d0798f543",
            ),
            ArchiveHash::new(
                "ZH_Generals/gensec.big",
                "b3baa19899073a3b7ef5fe4347137a09a3f0ef5613175538ccd2b641f9734c9a",
            ),
            ArchiveHash::new(
                "ZH_Generals/INI.big",
                "bff8d621088b25fd8b041c8acca020a020fabc66f972ab2bd131fc67d905a72c",
            ),
            ArchiveHash::new(
                "ZH_Generals/maps.big",
                "8a241df0c87ea47f6ad992984ea73dabf4f2ac5f96f8924f30bb0bf5fc4ac4d1",
            ),
            ArchiveHash::new(
                "ZH_Generals/Music.big",
                "c1e162b8a7575d98d9e20c7ef582e3edc96b35e3d071d821b01c5426d3c55450",
            ),
            ArchiveHash::new(
                "ZH_Generals/Patch.big",
                "28dc194412f96dc1f66412430cf74f2d89ad0cdabf70d2c8d1179d8e51743494",
            ),
            ArchiveHash::new(
                "ZH_Generals/shaders.big",
                "b982d3a99c8fae32a6d07ab0994274c1754a0fb08f69646f96d698b3986fe2a5",
            ),
            ArchiveHash::new(
                "ZH_Generals/Speech.big",
                "5106e92a91b1159fd861d5e4475beb6e7e05b0d5ac43d65520e02d89e39e33d4",
            ),
            ArchiveHash::new(
                "ZH_Generals/SpeechEnglish.big",
                "b48ede709de86437a9cff23bd27586a03f86e2b009e0773816af48496bb10ae2",
            ),
            ArchiveHash::new(
                "ZH_Generals/Terrain.big",
                "4c203b31ccbf7f4a41ca0288d3e782a356a0f75f3315c05a83d7364e19f86f71",
            ),
            ArchiveHash::new(
                "ZH_Generals/Textures.big",
                "1303e92c57c9cf4e24bf85b342bd58799924565796f3a4aef65dd4a9967aad5b",
            ),
            ArchiveHash::new(
                "ZH_Generals/W3D.big",
                "87727b698089cdc32bc378b1746bf315c2a920d5df33cace0d7085e027b67d36",
            ),
            ArchiveHash::new(
                "ZH_Generals/Window.big",
                "344f830ce00eabc247524b5e8b1305f10fe8c742a667e238d3a9c1c63fbe6479",
            ),
        ],
    }];

    /// The version of `known` whose archives `found` are, by their paths ignoring case and their
    /// SHA-256; otherwise how the first archive that differs from the first version differs.
    pub(crate) fn identify<'a>(
        found: &[HashedArchive],
        known: &'a [GameVersion],
    ) -> Result<&'a GameVersion, ImportError> {
        let found: BTreeMap<String, [u8; 32]> = found
            .iter()
            .map(|archive| (archive.name.to_ascii_lowercase(), archive.sha256))
            .collect();
        let mut first = None;
        for version in known {
            match version.difference(&found) {
                None => return Ok(version),
                Some(difference) => {
                    first.get_or_insert(difference);
                }
            }
        }
        let Difference {
            archive,
            difference,
        } = first.expect("the importer knows a version");
        Err(ImportError::ZeroHour(ZeroHourError::UnknownVersion {
            archive,
            difference,
        }))
    }

    /// The first archive, in the order of paths ignoring case, where `found` differs from this
    /// version, and how.
    fn difference(&self, found: &BTreeMap<String, [u8; 32]>) -> Option<Difference> {
        let ours: BTreeMap<String, [u8; 32]> = self
            .archives
            .iter()
            .map(|archive| (archive.path.to_ascii_lowercase(), archive.sha256))
            .collect();
        let names: BTreeSet<&String> = ours.keys().chain(found.keys()).collect();
        names.into_iter().find_map(|name| {
            let difference = match (ours.get(name), found.get(name)) {
                (Some(_), None) => VersionDifference::Missing,
                (None, Some(_)) => VersionDifference::Extra,
                (Some(ours), Some(found)) if ours != found => VersionDifference::OtherBytes,
                _ => return None,
            };
            Some(Difference {
                archive: name.clone(),
                difference,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_install_is_the_version_whose_every_archive_it_holds() {
        // A hash of 64 hex digits is its 32 bytes.
        let hash = ArchiveHash::new(
            "x.big",
            "00010203040506070809aaabacadaeaf101112131415161718191a1b1c1d1eff",
        );
        assert_eq!(hash.sha256[..3], [0x00, 0x01, 0x02]);
        assert_eq!(hash.sha256[10..12], [0xAA, 0xAB]);
        assert_eq!(hash.sha256[31], 0xFF);

        let archive = |path, byte| ArchiveHash {
            path,
            sha256: [byte; 32],
        };
        let found = |archives: &[(&str, u8)]| -> Vec<HashedArchive> {
            archives
                .iter()
                .map(|&(name, byte)| HashedArchive {
                    name: name.to_owned(),
                    sha256: [byte; 32],
                })
                .collect()
        };
        let known = [
            GameVersion {
                name: "one",
                archives: Box::leak(Box::new([archive("A.big", 1), archive("b.big", 2)])),
            },
            GameVersion {
                name: "two",
                archives: Box::leak(Box::new([archive("A.big", 1), archive("b.big", 3)])),
            },
        ];
        // Paths match ignoring case; the second version is found when the first is not.
        let identify = |archives| GameVersion::identify(&found(archives), &known);
        assert_eq!(identify(&[("a.BIG", 1), ("B.big", 2)]).unwrap().name, "one");
        assert_eq!(identify(&[("A.big", 1), ("b.big", 3)]).unwrap().name, "two");
        // Each difference, named against the first version, the first archive in path order.
        for (archives, archive, difference) in [
            (
                &[("A.big", 1), ("b.big", 2), ("c.big", 4)][..],
                "c.big",
                VersionDifference::Extra,
            ),
            (&[("A.big", 1)], "b.big", VersionDifference::Missing),
            (
                &[("A.big", 9), ("b.big", 9)],
                "a.big",
                VersionDifference::OtherBytes,
            ),
        ] {
            assert!(
                matches!(
                    identify(archives),
                    Err(ImportError::ZeroHour(ZeroHourError::UnknownVersion { archive: at, difference: how }))
                        if at == archive && how == difference
                ),
                "{archives:?}"
            );
        }

        // The known Steam version: 35 archives, none twice, the skipped duplicate not among them.
        let steam = &GameVersion::KNOWN[0];
        assert_eq!(steam.archives.len(), 35);
        let names: BTreeSet<String> = steam
            .archives
            .iter()
            .map(|archive| archive.path.to_ascii_lowercase())
            .collect();
        assert_eq!(names.len(), 35);
        assert!(!names.contains("data/ini/inizh.big"));
    }
}
