use super::*;
use crate::app_frame_interpolation_ext::AppFrameInterpolationExt;
use crate::frame_interpolation_history::FrameInterpolationHistory;
use crate::frame_registry_plugin::FrameRegistryPlugin;
use crate::frame_rule::FrameRule;

#[derive(Component, Clone, Debug, PartialEq)]
struct Sim(f32);

#[derive(Component, Clone, Debug, PartialEq)]
struct OtherSim(f32);

#[derive(Component, Clone, Debug, PartialEq)]
struct Shown(f32);

#[derive(Component, Clone, Debug, PartialEq)]
struct OtherShown(f32);

/// A registry records what each component is shown through, which a correction asks, and lists
/// each component's frame history once as it seals, however many rules show it.
#[test]
fn the_registry_knows_each_components_visual() {
    let mut app = App::new();
    app.add_plugins(FrameRegistryPlugin);
    app.register_frame_rule::<Sim, Shown, ()>(FrameRule::disabled());
    app.register_frame_rule::<Sim, Shown, With<OtherSim>>(FrameRule::disabled().priority(1));
    app.finish();
    let registry = app.world().resource::<FrameRegistry>();
    assert!(registry.maps::<Sim, Shown>());
    assert!(!registry.maps::<Sim, OtherShown>());
    assert!(!registry.maps::<OtherSim, Shown>());
    let history = app
        .world()
        .components()
        .component_id::<FrameInterpolationHistory<Sim>>();
    assert_eq!(
        registry.frame_history_component_ids(),
        [history.unwrap()].as_slice()
    );
}

#[test]
#[should_panic(expected = "is shown through two visual components")]
fn a_component_has_one_visual() {
    let mut app = App::new();
    app.register_frame_rule::<Sim, Shown, ()>(FrameRule::disabled());
    app.register_frame_rule::<Sim, OtherShown, ()>(FrameRule::disabled());
}

#[test]
#[should_panic(expected = "shows two components")]
fn a_visual_shows_one_component() {
    let mut app = App::new();
    app.register_frame_rule::<Sim, Shown, ()>(FrameRule::disabled());
    app.register_frame_rule::<OtherSim, Shown, ()>(FrameRule::disabled());
}
