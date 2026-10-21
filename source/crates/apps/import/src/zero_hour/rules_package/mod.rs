use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::num::{NonZeroU8, NonZeroU32, NonZeroU64};

use campfire_actions::ActionsData;
use campfire_codec::{Toml, TomlTable, TomlValue};
use campfire_combat::{ModeCombatData, ModeCombatPatch};
use campfire_common::{DeclaredName, Version};
use campfire_declarations::SectionWriter;
use campfire_mode::{
    ChoiceData, ListEntry, ModeData, ModeParam, ModePatch, Offers, OptionData, RelationData,
    TeamData, TeamManifest,
};
use campfire_package::{
    Dependency, DependencyPath, Language, ModeManifest, PackageHeader, PackageName, ScriptOrder,
    ScriptSource, TickRange,
};
use campfire_script::{ApiVersion, ScriptLimits};
use campfire_sim::Capability;
use campfire_stats::ModifiersData;
use campfire_values::{PackagePath, Relation, Scalar, Speed, TextId};

use crate::zero_hour::ZeroHour;
use crate::zero_hour::combat_import::CombatImport;
use crate::zero_hour::format::ini::multiplayer_ini::MoneyChoices;
use crate::zero_hour::map_import::SLOTS;
use crate::zero_hour::production_import::ProductionImport;
use crate::zero_hour::skirmish_start::{FactionStart, SkirmishStart};

/// How many of each unit type a seat that starts with a base gets (09-zero-hour.md, A base
/// start).
const BASE_COUNT: i64 = 2;

/// Zero Hour's rules package as the importer ships it, `source/packages/zero-hour/`: each file but
/// its index, by its path, which the import writes beside the imported package.
pub(crate) const RULES_FILES: [(&str, &[u8]); 3] = [
    (
        "manifest.toml",
        include_bytes!("../../../../../../packages/zero-hour/manifest.toml"),
    ),
    (
        "data/rules.toml",
        include_bytes!("../../../../../../packages/zero-hour/data/rules.toml"),
    ),
    (
        "scripts/mode.rhai",
        include_bytes!("../../../../../../packages/zero-hour/scripts/mode.rhai"),
    ),
];

/// The imported package's manifest: a mode of eight players, each on a team of its own, beside the
/// `neutral` team every placed object stands on, whose units drive, are made and fight with shells and
/// missiles: of `stats`, `orders`, `navigation`, `abilities`, `production`, `combat` and `projectiles`, no
/// unit faster than `max_move_speed`, at the game's 30 logic frames a second, depending on the
/// rules package beside it (03-game-scripting.md, The manifest writes as it reads).
pub(crate) fn mode_manifest(max_move_speed: Speed) -> ModeManifest<Vec<Capability>> {
    let team = |name: &str, seats| TeamManifest {
        name: DeclaredName::new(name).expect("a team's name is a declared name"),
        seats,
    };
    let teams = (1..=SLOTS)
        .map(|player| team(&format!("player_{player}"), 1))
        .chain([team("neutral", 0)])
        .collect();
    let package = |name| PackageName::new(name).expect("a package's name");
    let logic_frames = NonZeroU32::new(30).expect("30 is no zero");
    let dependency = Dependency {
        path: DependencyPath::parse("../zero-hour").expect("a path beside the package"),
    };
    ModeManifest {
        header: PackageHeader {
            name: package("zero-hour-game"),
            version: Version::new(0, 1, 0),
            api: ApiVersion::RELEASE,
            language: Language::parse("en").expect("English is a language"),
        },
        capabilities: vec![
            Capability::Stats,
            Capability::Orders,
            Capability::Navigation,
            Capability::Abilities,
            Capability::Production,
            Capability::Combat,
            Capability::Projectiles,
        ],
        tick_hz: TickRange::new(logic_frames, logic_frames, logic_frames)
            .expect("a range of one rate holds it"),
        teams,
        max_move_speed,
        // A match start of eight seats that start with bases spawns some 40 units for each, at
        // about 10 operations a unit (09-zero-hour.md, A base start).
        script_limits: ScriptLimits {
            per_call: NonZeroU64::new(8000).expect("8000 is no zero"),
            player: 8000,
            think: 8000,
            mode: 8000,
        },
        dependencies: BTreeMap::from([(package("zero-hour"), dependency)]),
    }
}

/// What the mode's data is made of: the skirmish's start, the trains and builds, the weapons and
/// armors, and the starting money.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ModeImport<'a> {
    pub(crate) start: &'a SkirmishStart,
    pub(crate) production: &'a ProductionImport,
    pub(crate) combat: &'a CombatImport,
    pub(crate) start_money: &'a MoneyChoices,
}

/// The imported package's mode data: what the install gives, over the rules package's fixed
/// data (03-game-scripting.md, Overrides by load order): its name and its teams' names, as its
/// manifest names the teams, the rules package's mode script, its params of what each faction
/// starts with and its choice of a faction, `start` (09-zero-hour.md, A skirmish starts with the
/// faction's start objects; 09-zero-hour.md, Each player picks a faction), each faction's base
/// and its choice of a start, standard or a base (09-zero-hour.md, A seat picks its start;
/// 09-zero-hour.md, A base start), and the
/// option of the money it starts with, of `start_money`'s values (09-zero-hour.md, The import
/// writes Zero Hour's money as an option); the armors, the weapons and the modifiers of their
/// stages of `combat` (09-zero-hour.md) and the trains and builds of `production`
/// (04-capabilities/production.md).
pub(crate) fn mode_data(mode: ModeImport<'_>) -> String {
    let ModeImport {
        start,
        production,
        combat,
        start_money,
    } = mode;
    debug_assert!(
        combat.actions.keys().all(|name| {
            !production.actions.contains_key(name) && !production.supply.gathers.contains_key(name)
        }),
        "a weapon, a train or build and a gather share no name, as one table holds them"
    );
    let declared = |name: &str| DeclaredName::new(name).expect("a name the import gives");
    let text = |id: &str| TextId::new(id).expect("a message id the import gives");
    let teams = (1..=SLOTS)
        .map(|player| (format!("player_{player}"), format!("team-player-{player}")))
        .chain([(String::from("neutral"), String::from("team-neutral"))])
        .map(|(team, name)| (declared(&team), TeamData { name: text(&name) }))
        .collect();
    let param =
        |name: &str, faction: &FactionStart| declared(&format!("{name}_{}", faction.name.as_str()));
    let list = |names: &[DeclaredName]| {
        ModeParam::List(
            names
                .iter()
                .map(|name| ListEntry::Text(name.to_string()))
                .collect(),
        )
    };
    let params = start.factions.iter().flat_map(|faction| {
        let base = production.base(&faction.building, &faction.units);
        [
            (
                param("start_building", faction),
                ModeParam::Text(faction.building.to_string()),
            ),
            (param("start_units", faction), list(&faction.units)),
            (
                param("start_offset", faction),
                ModeParam::Value(Scalar::Decimal(faction.offset)),
            ),
            (param("base_buildings", faction), list(&base.buildings)),
            (param("base_units", faction), list(&base.units)),
        ]
    });
    let base_count = (
        declared("base_count"),
        ModeParam::Value(Scalar::Int(BASE_COUNT)),
    );
    let params = params.chain([base_count]).collect();
    let start_choice = ChoiceData {
        name: text("choice-start"),
        offers: Offers::Values,
        unique: false,
        count: NonZeroU8::MIN,
        slot: None,
        values: ["standard", "base"].map(declared).into(),
    };
    let faction = ChoiceData {
        name: text("choice-faction"),
        offers: Offers::Values,
        unique: false,
        count: NonZeroU8::MIN,
        slot: None,
        values: start
            .factions
            .iter()
            .map(|faction| faction.name.clone())
            .collect(),
    };
    let money = start_money
        .values
        .iter()
        .map(|&value| Scalar::Int(i64::from(value)));
    let money = OptionData::new(money.collect(), Scalar::Int(i64::from(start_money.default)))
        .expect("the multiplayer settings' read checks that their choices make an option");
    let rules = ScriptSource::Rules {
        package: PackageName::new(ZeroHour::RULES).expect("the rules package's name"),
        path: PackagePath::parse("scripts/mode.rhai").expect("the rules' mode script's path"),
    };
    // Each player regards the neutral team as neutral, as `PlayerList::newGame` sets every
    // player's relation to the neutral player (09-zero-hour.md, Players regard the
    // neutral team as neutral).
    let relations = (1..=SLOTS).map(|player| RelationData {
        teams: [declared(&format!("player_{player}")), declared("neutral")],
        relation: Relation::Neutral,
        vision: false,
    });
    let mut writer = SectionWriter::default();
    writer.key(ScriptOrder::KEY, &ScriptOrder::one(rules));
    writer.own::<ModeData>(&ModePatch {
        name: Some(text("mode-name")),
        teams: Some(teams),
        relations: Some(relations.collect()),
        params: Some(params),
        options: Some(BTreeMap::from([(declared("start_money"), money)])),
        choices: Some(BTreeMap::from([
            (declared("faction"), faction),
            (declared("start"), start_choice),
        ])),
        ..ModePatch::default()
    });
    if !combat.armors.is_empty() {
        writer.patch::<ModeCombatData>(&ModeCombatPatch {
            armors: Some(combat.armors.clone()),
            ..ModeCombatPatch::default()
        });
    }
    let weapons = combat
        .actions
        .iter()
        .map(|(id, weapon)| (id, weapon.table()));
    let trains = production
        .actions
        .iter()
        .map(|(id, train)| (id, train.table()));
    let gathers = production
        .supply
        .gathers
        .iter()
        .map(|(id, gather)| (id, gather.table()));
    let actions: BTreeMap<&DeclaredName, TomlTable> =
        weapons.chain(trains).chain(gathers).collect();
    if !actions.is_empty() {
        writer.value::<ActionsData>(TomlValue::of(&actions));
    }
    if !combat.modifiers.is_empty() {
        let modifiers = combat
            .modifiers
            .iter()
            .map(|(id, stage)| (id, stage.table()));
        let modifiers: BTreeMap<&DeclaredName, TomlValue> = modifiers.collect();
        writer.value::<ModifiersData>(TomlValue::of(&modifiers));
    }
    Toml::write(&writer.into_table())
}

/// The imported package's text in its own language: the names its mode data gives, and each of
/// `texts`, an action's name by its message.
pub(crate) fn mode_text(texts: &BTreeMap<String, String>) -> String {
    let mut text =
        String::from("mode-name = Zero Hour\nchoice-faction = Faction\nchoice-start = Start\n");
    for player in 1..=SLOTS {
        writeln!(text, "team-player-{player} = Player {player}")
            .expect("text writes into a string");
    }
    text.push_str("team-neutral = Neutral\n");
    for (id, value) in texts {
        writeln!(text, "{id} = {}", fluent_value(value)).expect("text writes into a string");
    }
    text
}

/// `text` as a Fluent message's value that gives it back: each line past the first indented, and
/// as a string literal each of what Fluent reads as syntax or drops: a brace, a later line's
/// leading `[`, `*` or `.`, the spaces that start or end a line, and an empty line.
fn fluent_value(text: &str) -> String {
    let mut value = String::with_capacity(text.len());
    for (at, line) in text.split('\n').enumerate() {
        if at > 0 {
            value.push_str("\n    ");
        }
        let core = line.trim_matches(' ');
        if core.is_empty() {
            push_literal(&mut value, line);
            continue;
        }
        let lead = &line[..line.len() - line.trim_start_matches(' ').len()];
        let trail = &line[line.trim_end_matches(' ').len()..];
        if !lead.is_empty() {
            push_literal(&mut value, lead);
        }
        for (place, char) in core.char_indices() {
            match char {
                '{' | '}' => push_literal(&mut value, &core[place..=place]),
                '[' | '*' | '.' if place == 0 && at > 0 && lead.is_empty() => {
                    push_literal(&mut value, &core[place..=place]);
                }
                _ => value.push(char),
            }
        }
        if !trail.is_empty() {
            push_literal(&mut value, trail);
        }
    }
    value
}

/// Pushes `text`, of no quote or backslash, as a Fluent string literal.
fn push_literal(value: &mut String, text: &str) {
    write!(value, r#"{{"{text}"}}"#).expect("text writes into a string");
}

#[cfg(test)]
mod tests;
