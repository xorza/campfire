use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

use campfire_common::DeclaredName;
use campfire_script::{
    IdNames, ScriptHandle, ScriptList, ScriptText, ScriptValue, plain_script_arg,
};
use campfire_values::{NameId, Names, PathId};

use crate::marker::{Marker, MarkerInfo};
use crate::marker_spec::MarkerSpec;
use crate::marker_tag::MarkerTag;

/// `ctx.map`: the map's paths, by name, and its markers, in the map's order, with the tags they
/// hold. A clone shares them, as each `ctx.map` read gives one.
#[derive(Debug, Clone, Default)]
pub(crate) struct GameMap(Rc<MapFields>);

/// The fields of a `GameMap`.
#[derive(Debug, Default)]
struct MapFields {
    paths: ScriptList,
    markers: Box<[Marker]>,
    /// The tags its markers hold.
    tags: Arc<Names<MarkerTag>>,
    /// The places in `markers` of the markers of each tag, a run for each tag in the order of
    /// the tags, each run in the map's order.
    by_tag: Box<[u32]>,
    /// Where each tag's run starts in `by_tag`, then where the last one ends.
    starts: Box<[u32]>,
}

impl GameMap {
    /// The map of `paths`, by name, and `markers`, their params as scripts read them; its tags
    /// are its markers' and the mode's `spawn_tags`, which a script may name on a map with no
    /// marker of one, as a mode names the markers it spawns at whatever map it plays.
    pub(crate) fn new(
        paths: impl Iterator<Item = ScriptText>,
        markers: &[MarkerSpec],
        spawn_tags: &[DeclaredName],
    ) -> Self {
        let names: BTreeSet<&str> = markers
            .iter()
            .flat_map(|marker| marker.tags.iter().map(DeclaredName::as_str))
            .chain(spawn_tags.iter().map(DeclaredName::as_str))
            .collect();
        let tags = Names::all_of(names).expect("each tag once, below the limit");
        let mut held: Vec<(MarkerTag, u32)> = Vec::new();
        for (at, marker) in markers.iter().enumerate() {
            let at = u32::try_from(at).expect("markers fit u32");
            for tag in &marker.tags {
                held.push((tags.named(tag.as_str()).expect("a tag of the map"), at));
            }
        }
        held.sort_unstable();
        held.dedup();
        let mut starts = Vec::with_capacity(tags.len() + 1);
        starts.push(0);
        for tag in 0..tags.len() {
            let end = held.partition_point(|&(held, _)| held.index() <= tag);
            starts.push(u32::try_from(end).expect("markers fit u32"));
        }
        let markers = markers.iter().map(|marker| {
            let params = marker
                .params
                .iter()
                .map(|(name, param)| (name.as_str(), param.to_value()));
            Marker::new(MarkerInfo {
                name: marker.name.as_str().into(),
                position: marker.position,
                team: marker.team,
                params: params.collect(),
            })
        });
        Self(Rc::new(MapFields {
            paths: paths.map(ScriptValue::of).collect(),
            markers: markers.collect(),
            tags: Arc::new(tags),
            by_tag: held.into_iter().map(|(_, at)| at).collect(),
            starts: starts.into_boxed_slice(),
        }))
    }

    /// The tags its markers hold.
    pub(crate) fn tags(&self) -> &Arc<Names<MarkerTag>> {
        &self.0.tags
    }

    /// The paths' names, by path.
    pub(crate) fn paths(&self) -> IdNames<PathId> {
        IdNames::of_list(self.0.paths.clone())
    }

    /// The markers with tag `tag`, in the map's order.
    pub(crate) fn markers(&self, tag: MarkerTag) -> ScriptList {
        self.held(tag)
            .iter()
            .map(|&at| ScriptValue::of(self.0.markers[at as usize].clone()))
            .collect()
    }

    /// The places of the markers with tag `tag`, in the map's order.
    fn held(&self, tag: MarkerTag) -> &[u32] {
        let fields = &*self.0;
        &fields.by_tag[fields.starts[tag.index()] as usize..fields.starts[tag.index() + 1] as usize]
    }
}

plain_script_arg!(GameMap);

impl ScriptHandle for GameMap {
    const NAME: &'static str = "Map";
    const TITLE: &'static str = "Map, `ctx.map`";
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_math::Vec3;

    use crate::game_map::GameMap;
    use crate::internals::MarkerOf;

    impl GameMap {
        /// The point of the first marker of `of.tag`, in the map's order, whose param `of.param`
        /// is the text `of.value`; none for no such marker, or one of no point.
        pub(crate) fn point_of(&self, of: MarkerOf<'_>) -> Option<Vec3> {
            let tag = self.0.tags.named(of.tag)?;
            let named = self.held(tag).iter().find_map(|&at| {
                let info = self.0.markers[at as usize].info();
                let param = info.params.get(of.param)?.into_text()?;
                (param.as_str() == of.value).then_some(info)
            })?;
            named.position
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use campfire_common::DeclaredName;
    use campfire_script::ScriptText;

    use crate::game_map::GameMap;
    use crate::marker_spec::MarkerSpec;

    #[test]
    fn the_markers_of_a_tag_are_in_the_maps_order() {
        let name = |text: &str| DeclaredName::new(text).unwrap();
        let marker = |at: &str, tags: &[&str]| MarkerSpec {
            name: name(at),
            tags: tags.iter().map(|tag| name(tag)).collect(),
            position: None,
            region: None,
            team: None,
            params: BTreeMap::new(),
        };
        // `spawn` on markers 0 and 2, `camp` on 1 and 2, `exit` on 3 once though it names it
        // twice; marker 4 holds no tag.
        let markers = [
            marker("a", &["spawn"]),
            marker("b", &["camp"]),
            marker("c", &["spawn", "camp"]),
            marker("d", &["exit", "exit"]),
            marker("e", &[]),
        ];
        // `start`, a spawn tag no marker holds, is a tag too, of no marker.
        let spawn_tags = ["spawn", "start"].map(|tag| DeclaredName::new(tag).unwrap());
        let map = GameMap::new(std::iter::empty::<ScriptText>(), &markers, &spawn_tags);
        let tags = map.tags();
        assert_eq!(
            tags.texts().collect::<Vec<_>>(),
            ["camp", "exit", "spawn", "start"]
        );
        assert!(map.held(tags.named("start").unwrap()).is_empty());
        let held = |tag: &str| map.held(tags.named(tag).unwrap());
        assert_eq!(held("camp"), [1, 2]);
        assert_eq!(held("exit"), [3]);
        assert_eq!(held("spawn"), [0, 2]);
        assert_eq!(map.markers(tags.named("spawn").unwrap()).len(), 2);
    }
}
