use super::{NamedCode, Naming, capability, named_code};
use crate::workspace::Workspace;

/// Every lookup by name that the production code of the capabilities and the network calls, by
/// file: a method or function whose name ends in `named`, as each lookup of an id by its name is
/// spelled, a table's `named` the most. Each runs in a script call, which resolves the names it is
/// given once, as its arguments' types take them, in the load, which resolves the packages' names
/// once, or as a bot starts. The test fails when a lookup appears and when one listed here is gone.
const LOOKUPS: [(&str, &str); 89] = [
    // The load, each module's books by their builders.
    ("actions/action_load.rs", "named"),
    ("actions/action_names.rs", "named"),
    ("actions/action_rules.rs", "named"),
    ("actions/action_slots/mod.rs", "named"),
    ("actions/cost_target.rs", "named"),
    ("actions/listed_effect/effect_data.rs", "named"),
    ("actions/loaded_action.rs", "named"),
    ("actions/slot_claims.rs", "named"),
    ("actions/slot_kinds.rs", "named"),
    ("areas/area_spec.rs", "named"),
    ("books/load_names.rs", "named"),
    ("combat/armor.rs", "named"),
    ("combat/barrels/mod.rs", "named"),
    ("combat/combat_books.rs", "named"),
    ("combat/mode_combat_data.rs", "named"),
    ("combat/second_life.rs", "named"),
    ("combat/turret/mod.rs", "named"),
    ("combat/type_armor.rs", "named"),
    ("combat/weapon_names.rs", "named"),
    ("declarations/book_cx/mod.rs", "named"),
    ("interaction/enter_spec.rs", "named"),
    ("interaction/use_slot.rs", "named"),
    ("interaction/use_spec.rs", "named"),
    ("items/inventory.rs", "named"),
    ("items/item_book.rs", "named"),
    ("items/items_rules.rs", "named"),
    ("items/shop.rs", "named"),
    ("mode/game_map.rs", "named"),
    ("mode/mode_data.rs", "named"),
    ("mode/mode_map/mod.rs", "named"),
    ("mode/mode_units.rs", "named"),
    ("navigation/navigation_books.rs", "named"),
    ("production/book_builders.rs", "named"),
    ("production/requirement_book.rs", "named"),
    ("production/research_spec.rs", "named"),
    ("progression/experience_book.rs", "named"),
    ("progression/perk_book/mod.rs", "named"),
    ("progression/track_modifier_book.rs", "named"),
    ("progression/type_tracks.rs", "named"),
    ("script/scripts/script_facts/mod.rs", "hook_named"),
    ("stats/action_params.rs", "named"),
    ("stats/modifier_book.rs", "named"),
    ("stats/modifier_spec.rs", "named"),
    ("stats/modifiers_data.rs", "named"),
    ("stats/param_book.rs", "named"),
    ("stats/stat_book.rs", "named"),
    ("stats/type_passive.rs", "named"),
    ("stats/type_pools.rs", "named"),
    ("stats/type_upgrades.rs", "named"),
    ("units/collision_data.rs", "named"),
    ("units/declared_tags.rs", "named"),
    ("units/pool_id.rs", "named"),
    ("units/unit_types.rs", "named"),
    ("units/units_rules.rs", "named"),
    ("values/stat.rs", "named"),
    // The load of a sortie's stranded loss, its damage kind by name.
    ("airfields/sortie_data.rs", "named"),
    // The load, and the host's settings as the server starts.
    ("visibility/mode_visibility.rs", "named"),
    ("visibility/setting/mod.rs", "named"),
    // A bot's script, each unit type it names resolved once as its bot starts, and each action
    // a think's order names, by the action book's index of names.
    ("bots/bot_script/bot_names.rs", "actions_named"),
    ("bots/bot_script/bot_names.rs", "named"),
    // The load, and a filter a script names.
    ("names/filter.rs", "named"),
    ("names/match_names.rs", "named"),
    ("names/name_tables.rs", "named"),
    ("values/filter_data.rs", "named"),
    // A probe's unit type, as a command line or a test names it.
    ("match_probe/probe_spec.rs", "named"),
    // Script calls, each argument by its type, and the mode inputs, whose names enter with the
    // players' inputs.
    ("actions/slot_kind.rs", "named"),
    ("mode/bounty_book.rs", "named"),
    ("mode/choice_id.rs", "named"),
    ("mode/marker_tag.rs", "named"),
    ("mode/mode_api.rs", "loadout_ability_named"),
    ("mode/mode_api.rs", "offer_named"),
    ("mode/mode_call.rs", "param_named"),
    ("mode/mode_core.rs", "named"),
    ("mode/mode_hooks.rs", "input_type_named"),
    ("mode/mode_schema.rs", "named"),
    ("mode/mode_script.rs", "named"),
    ("mode/roster.rs", "named"),
    ("mode/timer_id.rs", "named"),
    ("mode/type_income.rs", "named"),
    ("mode/victory.rs", "named"),
    ("script/scripts/id_args.rs", "named"),
    ("script/scripts/script_api/api_builder.rs", "named"),
    ("script/scripts/script_api/mod.rs", "hook_named"),
    ("script/scripts/state_decl/state_schema.rs", "named"),
    ("names/tag_names.rs", "named"),
    ("stats/param_table.rs", "named"),
    ("stats/stats_call.rs", "named"),
    ("units/stat_id.rs", "named"),
    ("values/name_table.rs", "named"),
];

pub(crate) fn a_name_is_looked_up_only_by_a_script_call_or_the_load(workspace: &Workspace) {
    // The capability crates, and the bots' crate, whose scripts name unit types.
    let sources = named_code(workspace, |krate| {
        let bots = krate.path == "crates/session/bots";
        capability(krate).or_else(|| bots.then(|| Naming::Under("bots".to_owned())))
    });
    let mut found = Vec::new();
    for NamedCode { path, code } in sources {
        for line in code.lines() {
            for (at, _) in line.match_indices("named(") {
                let start = line[..at]
                    .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .map_or(0, |before| before + 1);
                let lookup = &line[start..at + "named".len()];
                if !line[..start].trim_end().ends_with("fn") {
                    found.push((path.clone(), lookup.to_owned()));
                }
            }
        }
    }
    found.sort();
    found.dedup();
    let mut known: Vec<(String, String)> = LOOKUPS
        .iter()
        .map(|&(file, lookup)| (file.to_owned(), lookup.to_owned()))
        .collect();
    known.sort();
    assert_eq!(found, known, "lookups by name");
}
