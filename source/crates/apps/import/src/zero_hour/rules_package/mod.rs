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
use campfire_orders::{ModeRetaliationData, ModeRetaliationPatch};
use campfire_package::{
    BotData, BotList, Dependency, Language, ModeManifest, PackageHeader, ScriptOrder, TickRange,
};
use campfire_package_format::DependencyPath;
use campfire_progression::{ModeExperienceData, ModePerksData, ModeTracksData};
use campfire_script::{ApiVersion, ScriptLimits};
use campfire_sim::Capability;
use campfire_stats::{ModeStatsData, ModifiersData};
use campfire_values::{PackageName, PackagePath, Relation, Scalar, ScriptSource, Speed, TextId};

use crate::zero_hour::combat_import::CombatImport;
use crate::zero_hour::contain_import::ContainImport;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::ai_data_ini::AiRetaliation;
use crate::zero_hour::format::ini::multiplayer_ini::MoneyChoices;
use crate::zero_hour::import_team::ImportTeam;
use crate::zero_hour::mover_import::MoverImport;
use crate::zero_hour::production_import::{FactionBase, ProductionImport};
use crate::zero_hour::rules_names::RulesNames;
use crate::zero_hour::science_import::ScienceImport;
use crate::zero_hour::skirmish_start::{FactionStart, SkirmishStart};
use crate::zero_hour::upgrade_import::UpgradeImport;
use crate::zero_hour::veterancy_import::VeterancyImport;

/// The operations of a script call, and of each pool's calls of a tick: twice the most one think
/// of the Zero Hour check's bots runs, rounded up, as a longer match's bases grow
/// (09-zero-hour.md, The bots' limits are measured).
const BOT_LIMIT: u64 = 50_000;

/// How many of each unit type a player slot that starts with a base gets (09-zero-hour.md, A base
/// start).
const BASE_COUNT: i64 = 2;

/// The name of the rules package, and of its directory in an import.
pub(crate) const RULES: &str = "zero-hour";

/// The mode's choice of a player's faction, whose value names the player's music.
pub(crate) const FACTION: &str = "faction";

/// Zero Hour's rules package as the importer ships it, `source/packages/zero-hour/`: each file but
/// its index, by its path, which the import writes beside the imported package.
pub(crate) const RULES_FILES: [(&str, &[u8]); 4] = [
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
    (
        "scripts/bot.rhai",
        include_bytes!("../../../../../../packages/zero-hour/scripts/bot.rhai"),
    ),
];

/// The mode's bots, both of the rules package's bot script, each thinking every 10 s, as a move
/// each second restarts the army's paths before it gets far, and each keeping 3 gatherers for
/// each drop-off: the `builder`, which builds its base and trains no army, and the `attacker`,
/// which also sends an army of 30 at the enemy, as 12 die against a base's defenses, guarding 400
/// round its start while it gathers, going at the nearest enemy it sees within 5,000, all of a map
/// with no vision, and attacking within 300 of it; each searches its builds' places on a grid of
/// the pathfind cell, 10, within 64 cells, as the mode's script finds a start's room
/// (09-zero-hour.md, A builder and an attacker; 04-capabilities/control.md, A bot gathers).
fn bots() -> BotList {
    let declared = |name: &str| DeclaredName::new(name).expect("a name the import gives");
    let script = ScriptSource::Rules {
        package: PackageName::new(RULES).expect("the rules package's name"),
        path: PackagePath::parse("scripts/bot.rhai").expect("the rules' bot script's path"),
    };
    let bot = |params: &[(&str, i64)]| BotData {
        script: script.clone(),
        think_ms: NonZeroU32::new(10000).expect("10 s is no zero"),
        params: params
            .iter()
            .map(|&(name, value)| (declared(name), ModeParam::Value(Scalar::Int(value))))
            .collect(),
    };
    let builder = [
        ("army", 0),
        ("gatherers", 3),
        ("place_gap", 10),
        ("place_steps", 64),
    ];
    let attacker = [
        ("army", 30),
        ("gatherers", 3),
        ("guard", 400),
        ("sight", 5000),
        ("reach", 300),
        ("place_gap", 10),
        ("place_steps", 64),
    ];
    BotList::new(BTreeMap::from([
        (declared("builder"), bot(&builder)),
        (declared("attacker"), bot(&attacker)),
    ]))
}

/// The imported package's manifest: a mode of eight players, each on a team of its own, beside the
/// `neutral` team every placed object stands on, whose units drive, are made and fight with shells
/// and missiles: of `stats`, `orders`, `navigation`, `abilities`, `production`, `combat` and
/// `projectiles`, no unit faster than `max_move_speed`, at the game's 30 logic frames a second,
/// depending on the rules package beside it (03-game-scripting.md, The manifest writes as it
/// reads).
pub(crate) fn mode_manifest(max_move_speed: Speed) -> ModeManifest<Vec<Capability>> {
    let teams = ImportTeam::all()
        .map(|team| TeamManifest {
            name: team.name(),
            player_slots: u32::from(team != ImportTeam::Neutral),
        })
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
            Capability::Airfields,
            Capability::Vision,
            Capability::Progression,
            Capability::Areas,
            Capability::Interaction,
        ],
        tick_hz: TickRange::new(logic_frames, logic_frames, logic_frames)
            .expect("a range of one rate holds it"),
        teams,
        max_move_speed,
        // A bot's think of the check's plays runs up to 21,840 operations, and each pool holds a
        // whole call (09-zero-hour.md, The bots' limits are measured).
        script_limits: ScriptLimits {
            per_call: NonZeroU64::new(BOT_LIMIT).expect("a limit is no zero"),
            player: BOT_LIMIT,
            think: BOT_LIMIT,
            mode: BOT_LIMIT,
        },
        dependencies: BTreeMap::from([(package("zero-hour"), dependency)]),
    }
}

/// What the mode's data is made of: the skirmish's start, the trains, builds and researches, the
/// weapons and armors, the upgrades, the starting money, and the modifiers of the movers' hurts.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ModeImport<'a> {
    pub(crate) start: &'a SkirmishStart,
    pub(crate) production: &'a ProductionImport,
    pub(crate) combat: &'a CombatImport,
    pub(crate) upgrades: &'a UpgradeImport,
    pub(crate) veterancy: &'a VeterancyImport,
    pub(crate) sciences: &'a ScienceImport,
    pub(crate) contains: &'a ContainImport,
    pub(crate) start_money: &'a MoneyChoices,
    pub(crate) retaliation: Option<&'a ModeRetaliationPatch>,
    pub(crate) movers: &'a MoverImport,
}

/// The mode's `[retaliation]` of `ai`'s values, its aggressors `names`' enemies on the ground, as
/// `shouldRetaliateAgainstAggressor` refuses an airborne one (combat-response.md, The import
/// converts what the choice and the answer read); none when `AIData` gives none of them, and an
/// error for a value the engine holds no number of.
pub(crate) fn retaliation(
    ai: AiRetaliation,
    names: &RulesNames,
) -> Result<Option<ModeRetaliationPatch>, ZeroHourError> {
    let (Some(distance), Some(radius), Some(chase_ms)) = (ai.distance, ai.radius, ai.chase_ms)
    else {
        return Ok(None);
    };
    let num = |value: f32| game_float::to_num(value).map_err(|_| ZeroHourError::Retaliation);
    Ok(Some(ModeRetaliationPatch {
        aggressors: Some(names.enemies_on_ground.clone()),
        distance: Some(num(distance)?),
        radius: Some(num(radius)?),
        chase_ms: Some(u64::try_from(chase_ms).map_err(|_| ZeroHourError::Retaliation)?),
    }))
}

/// The imported package's mode data: what the install gives, over the rules package's fixed
/// data (03-game-scripting.md, Overrides by load order): its name and its teams' names, as its
/// manifest names the teams, the rules package's mode script, its params of what each faction
/// starts with and its choice of a faction, `start` (09-zero-hour.md, A skirmish starts with the
/// faction's start objects; 09-zero-hour.md, Each player picks a faction), each faction's base
/// and its choice of a start, standard or a base (09-zero-hour.md, A player slot picks its start;
/// 09-zero-hour.md, A base start), and the
/// option of the money it starts with, of `start_money`'s values (09-zero-hour.md, The import
/// writes Zero Hour's money as an option); the armors, the weapons, the modifiers of their
/// stages and the stats of their damage bonuses of `combat` (09-zero-hour.md), the trains, builds
/// and researches of `production` (04-capabilities/production.md), the modifiers of `upgrades`,
/// each upgrade's and each effect's (09-zero-hour.md, Zero Hour's upgrade flags are tags and
/// stats), and those of the hurts of `movers` (driving.md, A hurt type may hold a modifier).
pub(crate) fn mode_data(mode: ModeImport<'_>) -> String {
    let ModeImport {
        start,
        production,
        combat,
        upgrades,
        veterancy,
        sciences,
        contains,
        start_money,
        retaliation,
        movers,
    } = mode;
    debug_assert!(
        combat.actions.keys().all(|name| {
            !production.actions.contains_key(name)
                && !production.supply.gathers.contains_key(name)
                && !production.supply.hacks.contains_key(name)
        }),
        "a weapon, a train or build, a gather and a hack share no name, as one table holds them"
    );
    let declared = |name: &str| DeclaredName::new(name).expect("a name the import gives");
    let text = |id: &str| TextId::new(id).expect("a message id the import gives");
    let teams = ImportTeam::all()
        .map(|team| {
            let name = team.text_id();
            (team.name(), TeamData { name })
        })
        .collect();
    let param =
        |name: &str, faction: &FactionStart| declared(&format!("{name}_{}", faction.name.as_str()));
    let list = |names: &[DeclaredName]| {
        ModeParam::List(names.iter().cloned().map(ListEntry::Name).collect())
    };
    let params = start.factions.iter().flat_map(|faction| {
        let perks = sciences
            .intrinsic
            .get(&faction.name)
            .map_or(&[][..], Vec::as_slice);
        let base = FactionBase::of(production, combat, faction, perks);
        let fixed = [
            (
                param("start_building", faction),
                ModeParam::Name(faction.building.clone()),
            ),
            (param("start_units", faction), list(&faction.units)),
            (
                param("start_offset", faction),
                ModeParam::Value(Scalar::Decimal(faction.offset)),
            ),
            (param("base_buildings", faction), list(&base.buildings)),
            (param("base_units", faction), list(&base.units)),
            (param("army_units", faction), list(&base.army)),
            (param("gatherers", faction), list(&base.gatherers)),
            (param("drop_offs", faction), list(&base.drop_offs)),
            (param("perks", faction), list(perks)),
        ];
        // Each skill set its own list, `skill_set_<n>_<faction>` from 1, as a param holds no list
        // of lists, and their names in order (generals.md, A bot buys its skill set).
        let sets = sciences
            .skill_sets
            .get(&faction.name)
            .map_or(&[][..], Vec::as_slice);
        let set_name = |at: usize| param(&format!("skill_set_{}", at + 1), faction);
        let named = (0..sets.len()).map(set_name).collect::<Vec<_>>();
        let sets = sets
            .iter()
            .enumerate()
            .map(move |(at, set)| (set_name(at), list(set)));
        fixed
            .into_iter()
            .chain([(param("skill_sets", faction), list(&named))])
            .chain(sets)
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
        package: PackageName::new(RULES).expect("the rules package's name"),
        path: PackagePath::parse("scripts/mode.rhai").expect("the rules' mode script's path"),
    };
    // Each player regards the neutral team as neutral, as `PlayerList::newGame` sets every
    // player's relation to the neutral player (09-zero-hour.md, Players regard the
    // neutral team as neutral).
    let relations = ImportTeam::players().map(|player| RelationData {
        teams: [player.name(), ImportTeam::Neutral.name()],
        relation: Relation::Neutral,
        vision: false,
    });
    let mut writer = SectionWriter::default();
    writer.key(ScriptOrder::KEY, &ScriptOrder::one(rules));
    writer.key(BotList::KEY, &bots());
    writer.own::<ModeData>(&ModePatch {
        name: Some(text("mode-name")),
        teams: Some(teams),
        relations: Some(relations.collect()),
        params: Some(params),
        options: Some(BTreeMap::from([(declared("start_money"), money)])),
        choices: Some(BTreeMap::from([
            (declared(FACTION), faction),
            (declared("start"), start_choice),
        ])),
        ..ModePatch::default()
    });
    if let Some(retaliation) = retaliation {
        writer.patch::<ModeRetaliationData>(retaliation);
    }
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
    let hacks = production.supply.hacks.iter().map(|(id, hack)| {
        (
            id,
            TomlValue::of(hack).read().expect("the keys write a table"),
        )
    });
    let entries = contains.entry_tables();
    let actions: BTreeMap<&DeclaredName, TomlTable> = weapons
        .chain(trains)
        .chain(gathers)
        .chain(hacks)
        .chain(entries)
        .collect();
    if !actions.is_empty() {
        writer.value::<ActionsData>(TomlValue::of(&actions));
    }
    if !combat.bonuses.is_empty() || !production.power_stats.is_empty() {
        let none = TomlTable::default();
        let bonuses = combat.bonuses.keys().chain(&production.power_stats);
        let stats: BTreeMap<_, _> = bonuses.map(|stat| (stat, &none)).collect();
        writer.value::<ModeStatsData>(TomlValue::of(&stats));
    }
    let tracks: BTreeMap<_, _> = veterancy
        .track
        .iter()
        .chain(&sciences.track)
        .cloned()
        .collect();
    if !tracks.is_empty() {
        writer.section(&ModeTracksData(tracks));
    }
    let shares: Vec<_> = veterancy
        .share
        .iter()
        .chain(&sciences.share)
        .cloned()
        .collect();
    if !shares.is_empty() {
        writer.section(&ModeExperienceData(shares));
    }
    if !sciences.perks.is_empty() {
        writer.section(&ModePerksData(sciences.perks.clone()));
    }
    let stages = combat
        .modifiers
        .iter()
        .map(|(id, stage)| (id, stage.table()));
    let nothing = TomlValue::from(TomlTable::default());
    let researched = upgrades.upgrades.iter().map(|id| (id, nothing.clone()));
    let effects = upgrades
        .effects
        .iter()
        .map(|(id, effect)| (id, effect.table()));
    let levels = veterancy
        .modifiers
        .iter()
        .map(|(id, modifier)| (id, TomlValue::of(modifier)));
    let perks = sciences.perks.values().map(|perk| {
        let given = production.power_modifiers.get(&perk.modifier);
        (
            &perk.modifier,
            given.map_or_else(|| nothing.clone(), TomlValue::of),
        )
    });
    let garrisons = contains
        .modifiers
        .iter()
        .map(|(id, modifier)| (id, TomlValue::of(modifier)));
    let hurts = movers
        .modifiers
        .iter()
        .map(|(id, modifier)| (id, TomlValue::of(modifier)));
    let modifiers: BTreeMap<&DeclaredName, TomlValue> = stages
        .chain(researched)
        .chain(effects)
        .chain(levels)
        .chain(perks)
        .chain(garrisons)
        .chain(hurts)
        .collect();
    debug_assert_eq!(
        modifiers.len(),
        combat.modifiers.len()
            + upgrades.upgrades.len()
            + upgrades.effects.len()
            + veterancy.modifiers.len()
            + sciences.perks.len()
            + contains.modifiers.len()
            + movers.modifiers.len(),
        "a stage's, an upgrade's, an effect's, a level's, a perk's, a garrison's and a hurt's \
         modifiers share no name"
    );
    if !modifiers.is_empty() {
        writer.value::<ModifiersData>(TomlValue::of(&modifiers));
    }
    Toml::write(&writer.into_table())
}

/// The imported package's text in its own language: the names its mode data gives, and each of
/// `texts`, an action's name by its message.
pub(crate) fn mode_text(texts: &BTreeMap<String, String>) -> String {
    let mut text =
        String::from("mode-name = Zero Hour\nchoice-faction = Faction\nchoice-start = Start\n");
    for team in ImportTeam::all() {
        writeln!(text, "{} = {}", team.text_id(), team.text()).expect("text writes into a string");
    }
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
