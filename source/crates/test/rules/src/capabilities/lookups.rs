use proc_macro2::Ident;
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Expr, ExprCall, ExprMethodCall, ExprPath, Macro, Token};

use crate::capabilities::{NamedFile, Naming, capability, in_capability, named_files};
use crate::parsed_code::ParsedCode;
use crate::workspace::Workspace;

/// The bots' crate, whose scripts name unit types, beside the capability crates.
const BOTS: &str = "crates/session/bots";

/// Every lookup by name that the production code of the capability crates and the bots' crate
/// calls, by file: a method or function whose last segment ends in `named`, with its generic
/// arguments or none, as each lookup of an id by its name is spelled, a table's `named` the most. Each runs in a script call, which resolves the names it is
/// given once, as its arguments' types take them, in the load, which resolves the packages' names
/// once, or as a bot starts. The test fails when a lookup appears and when one listed here is gone.
const LOOKUPS: [(&str, &str); 109] = [
    // The load, each module's books by their builders.
    ("actions/action_load.rs", "named"),
    ("actions/action_names.rs", "named"),
    ("actions/action_parts.rs", "cost_target_named"),
    ("actions/action_parts.rs", "named"),
    ("actions/action_parts.rs", "type_named"),
    ("actions/action_rules.rs", "named"),
    ("actions/action_slots/mod.rs", "named"),
    ("actions/amount.rs", "param_named"),
    ("actions/cost_target.rs", "named"),
    ("actions/effect_names.rs", "named"),
    ("actions/listed_effect/effect_data.rs", "named"),
    ("actions/loaded_action.rs", "named"),
    ("actions/slot_claims.rs", "named"),
    ("actions/slot_kinds.rs", "named"),
    ("actions/spawn_effect_data.rs", "named"),
    ("areas/area_spec.rs", "named"),
    ("areas/land_effect_data.rs", "named"),
    ("books/load_names.rs", "named"),
    ("combat/armor.rs", "named"),
    ("combat/barrels/mod.rs", "named"),
    ("combat/combat_books.rs", "named"),
    ("combat/damage_effect_data.rs", "named"),
    ("combat/mode_combat_data.rs", "named"),
    ("combat/second_life.rs", "named"),
    ("combat/turret/mod.rs", "named"),
    ("combat/type_armor.rs", "named"),
    ("combat/weapon_names.rs", "named"),
    ("declarations/book_cx/mod.rs", "named"),
    ("interaction/enter_spec.rs", "named"),
    ("interaction/enter_spec.rs", "type_named"),
    ("interaction/interaction_rules.rs", "type_named"),
    ("interaction/use_slot.rs", "named"),
    ("interaction/use_spec.rs", "named"),
    ("items/inventory.rs", "named"),
    ("items/item_book.rs", "named"),
    ("items/items_rules.rs", "named"),
    ("items/shop.rs", "named"),
    ("mode/game_map.rs", "named"),
    ("mode/mode_data.rs", "named"),
    ("mode/mode_map/mod.rs", "named"),
    ("mode/mode_map/mod.rs", "type_named"),
    ("mode/mode_units.rs", "named"),
    ("navigation/navigation_books.rs", "named"),
    ("production/book_builders.rs", "named"),
    ("production/book_builders.rs", "type_named"),
    ("production/build_book.rs", "type_named"),
    ("production/requirement_book.rs", "named"),
    ("production/requirement_book.rs", "type_named"),
    ("production/research_spec.rs", "named"),
    ("production/train_spec.rs", "type_named"),
    ("progression/experience_book.rs", "named"),
    ("progression/perk_book/mod.rs", "named"),
    ("progression/track_modifier_book.rs", "named"),
    ("progression/type_tracks.rs", "named"),
    ("progression/xp_effect_data.rs", "named"),
    ("script/scripts/script_facts.rs", "hook_named"),
    ("stats/action_params.rs", "named"),
    ("stats/modifier_book.rs", "named"),
    ("stats/modifier_effect_data.rs", "named"),
    ("stats/modifier_spec.rs", "named"),
    ("stats/modifiers_data.rs", "named"),
    ("stats/param_book.rs", "named"),
    ("stats/purge_effect_data.rs", "named"),
    ("stats/restore_effect_data.rs", "named"),
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
    ("names/kind_names.rs", "named"),
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
    ("script/scripts/view_api.rs", "param_named"),
    ("names/tag_names.rs", "named"),
    ("stats/param_table.rs", "named"),
    ("stats/stats_call.rs", "named"),
    ("units/stat_id.rs", "named"),
    ("values/name_table.rs", "named"),
];

pub(crate) fn a_name_is_looked_up_only_by_a_script_call_or_the_load(workspace: &Workspace) {
    // The capability crates, and the bots' crate, whose scripts name unit types.
    let sources = named_files(workspace, |krate| {
        let bots = krate.path == BOTS;
        capability(krate).or_else(|| bots.then(|| Naming::Under("bots".to_owned())))
    });
    let mut found = Vec::new();
    for NamedFile { path, file } in sources {
        found.extend((file.findings().lookups.iter()).map(|lookup| (path.clone(), lookup.clone())));
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

/// The lookups `production`, the production code of the file at `path`, a capability crate's or
/// the bots' crate's, calls: each method and each function whose last segment ends in `named`,
/// with its generic arguments or none, as `syn` parses the calls; none of another file.
pub(crate) fn lookups(path: &str, production: &ParsedCode<'_>) -> Vec<String> {
    let bots = path
        .strip_prefix(BOTS)
        .is_some_and(|rest| rest.starts_with('/'));
    // A file that names no lookup calls none, and its parse is skipped.
    if !(in_capability(path) || bots) || !production.code.contains("named") {
        return Vec::new();
    }
    let mut calls = Lookups::default();
    calls.visit_file(production.file());
    calls.0
}

/// The lookups a walk of one file found.
#[derive(Debug, Default)]
struct Lookups(Vec<String>);

impl Lookups {
    fn take(&mut self, name: &Ident) {
        let name = name.to_string();
        if name.ends_with("named") {
            self.0.push(name);
        }
    }
}

impl<'ast> Visit<'ast> for Lookups {
    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        self.take(&call.method);
        visit::visit_expr_method_call(self, call);
    }

    /// A macro's arguments, where they parse as expressions, as a script argument's lookup is
    /// written in one.
    fn visit_macro(&mut self, mac: &'ast Macro) {
        if let Ok(args) = mac.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated) {
            let mut inner = Self::default();
            for arg in &args {
                inner.visit_expr(arg);
            }
            self.0.extend(inner.0);
        }
    }

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if let Expr::Path(ExprPath { path, .. }) = &*call.func
            && let Some(last) = path.segments.last()
        {
            self.take(&last.ident);
        }
        visit::visit_expr_call(self, call);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lookup_is_each_call_whose_last_segment_ends_in_named() {
        // A method, a turbofish's, a path's, and a wrapper of another name than `named` are each
        // a lookup; a function's own name, and a call of no such name, are none.
        let code = "fn named(x: u8) { a.named(\"x\"); b.named::<T>(\"y\"); Names::type_named(c); \
                    d.unit_type(e); f(named); arg!(Id, |ctx, name| ctx.param_named(name)); }";
        assert_eq!(
            lookups(
                "crates/session/bots/src/a.rs",
                &ParsedCode::new("a.rs", code)
            ),
            ["named", "named", "type_named", "param_named"]
        );
    }
}
