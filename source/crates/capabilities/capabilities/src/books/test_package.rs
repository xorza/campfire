use campfire_codec::{Toml, TomlTable};
use campfire_declare::{SourceKind, SourcePackage, UnitDefaults};
use campfire_math::Num;
use campfire_mode::{MapData, ModeData, TeamManifest};
use campfire_script::ScriptUses;
use campfire_sim::MatchRate;
use campfire_units::ScriptBook;
use campfire_values::Speed;

use crate::books::Books;
use crate::books::book_input::BookInput;
use crate::books::package_content::PackageContent;
use crate::release::Release;

/// A test's mode package as the text of its files, each a TOML table the release's sections
/// read: the mode's own, its map's, and its content, its unit types, actions and modifiers. It
/// names no script and no other package.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TestPackage<'a> {
    pub(crate) mode: &'a str,
    pub(crate) map: &'a str,
    pub(crate) content: &'a str,
}

impl TestPackage<'_> {
    /// The books of its match at `rate`, of `teams`, each built by the module that declares it,
    /// as a match's are; its stats refresh in the order the mode declares them, as no stat reads
    /// another.
    pub(crate) fn books(self, rate: MatchRate, teams: &[TeamManifest]) -> Books {
        let readers = Release::sections();
        let table = |text: &str| Toml::parse::<TomlTable>(text).unwrap();
        let data = table(self.mode)
            .read_with(ModeData::seed(&readers))
            .unwrap();
        let map = table(self.map).read_with(MapData::seed(&readers)).unwrap();
        let content = table(self.content)
            .read_with(PackageContent::seed(&readers, &UnitDefaults::default()))
            .unwrap();
        let mode = SourcePackage {
            name: "mode",
            kind: SourceKind::Mode,
            units: &content.units,
            content: &content.sections,
            scripts: Vec::new(),
        };
        let input = BookInput {
            data: &data,
            map: &map,
            heights: None,
            teams,
            max_move_speed: Speed::new(Num::MAX).unwrap(),
            set: Release::capabilities(),
            engine_tags: &Release::engine_tags(),
            packages: vec![mode],
            scripts: &ScriptBook::new(Release::hooks()),
            script_uses: &ScriptUses::default(),
            mode_scripts: Vec::new(),
            rate,
        };
        Release::books(&input).unwrap()
    }
}
