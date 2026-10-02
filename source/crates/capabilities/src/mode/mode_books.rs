use std::sync::Arc;

use bevy_ecs::world::World;

use crate::combat::combat_bindings::CombatBindings;
use crate::mode::mode_data::ModeData;
use crate::mode::mode_setup::UnitTypeSetup;
use crate::stats::Stats;
use crate::stats::pool_book::PoolBook;
use crate::stats::stat_book::StatBook;
use crate::units::Units;
use crate::units::body::Body;
use crate::units::script_view::View;
use crate::units::tag_book::TagBook;
use crate::units::unit_types::UnitTypes;
use crate::values::declared_name::DeclaredName;

/// The books of the mode's own rules: its stats, at the match's rate, under its move speed cap;
/// its pools; its tags' effects and each unit type's own tags; what its combat reads; and the
/// names of its damage kinds and of its players' resources, by id.
#[derive(Debug)]
pub struct ModeBooks {
    pub(crate) stats: StatBook,
    pub(crate) pools: PoolBook,
    pub(crate) tags: TagBook,
    /// None with no life pool, as in a mode with no combat.
    pub(crate) bindings: Option<CombatBindings>,
    pub(crate) damage_kinds: Arc<[DeclaredName]>,
    pub(crate) resources: Arc<[DeclaredName]>,
}

impl ModeBooks {
    /// Puts the books in `world`, a match whose capabilities are installed: the stat and pool
    /// books, the tags' effects, what combat reads, and the names its scripts read. `Mode::install`
    /// calls it; a test arena calls it alone, for a match whose mode runs no script.
    pub fn install(self, world: &mut World) {
        let ModeBooks {
            stats,
            pools,
            tags,
            bindings,
            damage_kinds,
            resources,
        } = self;
        world
            .non_send::<View>()
            .set_mode_names(&damage_kinds, resources);
        if let Some(bindings) = bindings {
            world.insert_resource(bindings);
        }
        Stats::load(world, stats, pools);
        Units::load_tags(world, tags);
    }

    /// The books of `data`, which the package load checked, for `unit_types`, the mode's unit
    /// types that stand, with the stat book `stats`. Each unit type is tagged with the name of
    /// the layer it moves on, among `types`, when the mode names its layers.
    pub(crate) fn build(
        data: &ModeData,
        unit_types: &[UnitTypeSetup],
        types: &mut UnitTypes,
        stats: StatBook,
    ) -> ModeBooks {
        let layers = &data.navigation.layers;
        for unit_type in unit_types {
            let layer = Body::layer_of(unit_type.kit.body.as_ref());
            if let Some(name) = layers.get(usize::from(layer.index())) {
                let tag = types
                    .tag_named(name.as_str())
                    .expect("the match declared every tag its packages name");
                types.give_tag(unit_type.unit_type, tag);
            }
        }
        ModeBooks {
            pools: PoolBook::new(&data.pools, &stats),
            bindings: CombatBindings::new(&data.combat, &data.pools, &stats),
            tags: types.tag_book(&data.tags),
            stats,
            damage_kinds: data.combat.damage_kinds.as_slice().into(),
            resources: data.resources.as_slice().into(),
        }
    }
}
