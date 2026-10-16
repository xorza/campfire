use campfire_common::MapName;
use campfire_math::Num;
use campfire_values::{DeclaredName, PackagePath};
use serde::{Deserialize, Serialize};

use crate::client_path;

/// A map's `client/maps/<name>/objects.toml`: the objects the client alone draws, still, of which
/// the sim holds none (12-zero-hour.md, The multiplayer rule for client objects). Each names its
/// look among the `objects` of its package's `client/units.toml`, and stands as a placed unit does:
/// at a point of the ground plane, a height above the ground there, and an angle in degrees, each
/// an exact decimal.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientObjects {
    pub objects: Vec<ClientObject>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientObject {
    pub look: DeclaredName,
    pub pos: [Num; 2],
    pub height: Num,
    pub angle: Num,
}

impl ClientObjects {
    /// The file of the map `map`.
    pub fn path(map: &MapName) -> PackagePath {
        client_path::of_map(map, "objects.toml")
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn objects_write_and_read_back_their_looks_and_exact_places() {
        let object = ClientObject {
            look: DeclaredName::new("ptoak01").unwrap(),
            pos: [Num::int(30), Num::int(-5) / Num::int(2)],
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
        let whole = read("[[objects]]\nlook = \"a\"\npos = [1, \"2.5\"]\nheight = 0\nangle = 90\n");
        let whole = whole.unwrap();
        assert_eq!(
            whole.objects[0].pos,
            [Num::int(1), Num::int(5) / Num::int(2)]
        );
        assert!(
            read("[[objects]]\nlook = \"a\"\npos = [1, 2]\nheight = 0\nangle = 0\nscale = 1\n")
                .is_err()
        );
    }
}
