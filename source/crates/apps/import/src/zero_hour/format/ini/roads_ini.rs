use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniText};

/// The road types of Zero Hour's `Roads.ini` files, as `TerrainRoadCollection` reads them, in the
/// order they are read; its bridges, which the import places as units, it counts in the ids and
/// passes over.
#[derive(Debug, Default)]
pub(crate) struct RoadsIni {
    roads: Vec<RoadIni>,
    /// The roads and bridges read, whose count numbers the next.
    read: u32,
}

/// A road type: its id, the game's, counting the roads and bridges from 1 in the order they are
/// read (`TerrainRoadCollection::m_idCounter`); its name, as the INI writes it, which a map's road
/// names exactly; its texture's file; `RoadWidth`, in world units; and `RoadWidthInTexture`, the
/// share of its width the texture draws across it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoadIni {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) texture: String,
    pub(crate) width: f32,
    pub(crate) width_in_texture: f32,
}

/// A road type of a `RoadsIni`, by its place in the order the types are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RoadId(pub(crate) usize);

/// The type whose fields a new type starts with.
const DEFAULT: &str = "DefaultRoad";
/// The id of the type the game gives a segment before it looks its template up
/// (`W3DRoadBuffer::addMapObjects`).
const UNTYPED: u32 = 1;

impl RoadsIni {
    /// Reads one INI file, adding its road types to those read before: a new one starts with
    /// `DefaultRoad`'s fields, or with none, and a name read before is an error, as
    /// `parseTerrainRoadDefinition` throws on it.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes).lines().blocks(BlockKinds::Any) {
            let block = block?;
            match block.head.word() {
                "road" => {}
                "bridge" => {
                    self.read += 1;
                    continue;
                }
                _ => continue,
            }
            let name = block.name()?;
            if self.named(name).is_some() {
                return Err(IniError::Redefined {
                    line: block.head.number,
                });
            }
            self.read += 1;
            let mut road = self.named(DEFAULT).map_or_else(
                || RoadIni {
                    id: 0,
                    name: String::new(),
                    texture: String::new(),
                    width: 0.0,
                    width_in_texture: 0.0,
                },
                |id| self[id].clone(),
            );
            road.id = self.read;
            name.clone_into(&mut road.name);
            for line in &block.fields {
                match line.word() {
                    "texture" => line.value()?.clone_into(&mut road.texture),
                    "roadwidth" => road.width = line.real()?,
                    "roadwidthintexture" => road.width_in_texture = line.real()?,
                    _ => {}
                }
            }
            self.roads.push(road);
        }
        Ok(())
    }

    /// The type a map's road of the template `template` is, its bytes Latin-1 chars matched of
    /// their case, as `findRoad` compares them; none for a name no type has.
    pub(crate) fn road(&self, template: &[u8]) -> Option<RoadId> {
        let name: String = template.iter().map(|&byte| char::from(byte)).collect();
        self.named(&name)
    }

    /// The type the game gives a map's road whose template no type names, the one of id 1
    /// (roads.md, A road's type is `Roads.ini`'s); none when that is a bridge, or no type.
    pub(crate) fn untyped(&self) -> Option<RoadId> {
        self.roads
            .iter()
            .position(|road| road.id == UNTYPED)
            .map(RoadId)
    }

    fn named(&self, name: &str) -> Option<RoadId> {
        self.roads
            .iter()
            .position(|road| road.name == name)
            .map(RoadId)
    }
}

impl std::ops::Index<RoadId> for RoadsIni {
    type Output = RoadIni;

    fn index(&self, RoadId(at): RoadId) -> &RoadIni {
        &self.roads[at]
    }
}

#[cfg(test)]
pub(crate) mod samples {
    /// Two road types, a gravel road and a sidewalk, and a bridge.
    pub(crate) const ROADS: &str = "Road GravelRoad\n  Texture = TRGravelRoad.tga\n  \
        RoadWidth = 35.0\n  RoadWidthInTexture = 0.9\nEnd\nBridge Wooden\n  Texture = W.tga\n\
        End\nRoad Sidewalk\n  Texture = TRSidewalk.tga\n  RoadWidth = 24\n  \
        RoadWidthInTexture = .95\nEnd\n";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::format::ini::roads_ini::samples::ROADS;

    #[test]
    fn a_road_takes_its_fields_the_default_s_first_and_a_name_read_again_is_refused() {
        let mut ini = RoadsIni::default();
        ini.read(ROADS.as_bytes()).unwrap();
        assert_eq!(ini.roads.len(), 2);
        let gravel = ini.road(b"GravelRoad").unwrap();
        assert_eq!(gravel, RoadId(0));
        assert_eq!(
            ini[gravel],
            RoadIni {
                id: 1,
                name: "GravelRoad".to_owned(),
                texture: "TRGravelRoad.tga".to_owned(),
                width: 35.0,
                width_in_texture: 0.9,
            }
        );
        // The bridge read between them takes id 2, so the sidewalk 3.
        let sidewalk = &ini[ini.road(b"Sidewalk").unwrap()];
        assert_eq!((sidewalk.id, sidewalk.width_in_texture), (3, 0.95));
        // A road of no type is the type of id 1, the gravel road.
        assert_eq!(ini.untyped(), Some(gravel));
        // Names match of their case; a bridge is no road.
        assert_eq!(ini.road(b"gravelroad"), None);
        assert_eq!(ini.road(b"Wooden"), None);
        assert_eq!(ini.road(b""), None);

        // A type read after `DefaultRoad` starts with its fields, one read before with none.
        let mut ini = RoadsIni::default();
        ini.read(b"Road Before\nEnd\nRoad DefaultRoad\n  Texture = D.tga\n  RoadWidth = 48\n  RoadWidthInTexture = 0.8\nEnd\nRoad After\n  RoadWidth = 30\nEnd\n")
            .unwrap();
        let fields = |name: &[u8]| {
            let road = &ini[ini.road(name).unwrap()];
            (road.texture.as_str(), road.width, road.width_in_texture)
        };
        assert_eq!(fields(b"Before"), ("", 0.0, 0.0));
        assert_eq!(fields(b"After"), ("D.tga", 30.0, 0.8));
        // Of a second file, the ids count on from the first's: 4 and 5.
        ini.read(b"Bridge B\nEnd\nRoad Later\nEnd\n").unwrap();
        assert_eq!(ini[ini.road(b"Later").unwrap()].id, 5);

        // A bridge first leaves no road of id 1.
        let mut ini = RoadsIni::default();
        ini.read(b"Bridge B\nEnd\nRoad R\nEnd\n").unwrap();
        assert_eq!(ini.untyped(), None);

        let fails = |text: &[u8]| RoadsIni::default().read(text).unwrap_err();
        assert_eq!(
            fails(b"Road A\nEnd\nRoad A\nEnd\n"),
            IniError::Redefined { line: 3 }
        );
        assert_eq!(fails(b"Road\nEnd\n"), IniError::NoName { line: 1 });
        assert!(matches!(
            fails(b"Road A\n  RoadWidth = wide\nEnd\n"),
            IniError::Real { line: 2, .. }
        ));
    }
}
