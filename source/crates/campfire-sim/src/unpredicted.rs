use bevy_ecs::component::Component;

/// Marks a unit a world holds as its authority sent it, and does not simulate: on a predicting
/// client, every replicated unit its player does not control. It hides the unit from every query
/// that does not name it, so the sim runs only on what the world predicts; the rest holds the
/// server's state as it arrives. Collision names it, to part the predicted units from the held
/// ones, and presentation reads these units with `Allow<Unpredicted>`.
#[derive(Component, Debug)]
pub struct Unpredicted;
