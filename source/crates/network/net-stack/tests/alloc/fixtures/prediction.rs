use crate::fixtures;
use crate::fixtures::AUDIT;
use crate::fixtures::LINK;
use bevy::prelude::{App, Entity, FixedUpdate, Query, With};
use campfire_net_connection::NetworkTarget;
use campfire_net_interpolation::internals::CompFull;
use campfire_net_prediction::PredictionHistory;
use campfire_net_replication::Predicted;
use campfire_net_replication::PredictionTarget;
use campfire_net_replication::Replicate;
use campfire_net_stack::internals::FullStack;
use campfire_net_stepper::ClientKind;
use campfire_net_stepper::Order;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperConfig;

fn increment(mut components: Query<'_, '_, &mut CompFull>) {
    for mut component in &mut components {
        component.0 += 1.0;
    }
}

fn increment_predicted(mut components: Query<'_, '_, &mut CompFull, With<Predicted>>) {
    for mut component in &mut components {
        component.0 += 1.0;
    }
}

fn value(app: &App, entity: Entity) -> f32 {
    app.world().get::<CompFull>(entity).unwrap().0
}

/// Frames for the client's prediction to move from the first replicated value to its place ahead
/// of the server.
const SETTLE: usize = 16;

/// Each tick the server and the predicting client both change one predicted component, and the
/// client records it in the prediction history. bevy_replicon grows its mutation message for the
/// change (1).
#[test]
fn prediction_updates() {
    audit_prediction(ClientKind::Raw);
}

/// The same in a sealed session of the UDP protocol.
#[test]
fn sealed_prediction_updates() {
    audit_prediction(ClientKind::Udp);
}

#[track_caller]
fn audit_prediction(kind: ClientKind) {
    let mut stepper = Stepper::new(FullStack, StepperConfig::with_clients(vec![kind]));
    stepper.server_app_mut().add_systems(FixedUpdate, increment);
    stepper
        .client_app_mut(0)
        .add_systems(FixedUpdate, increment_predicted);
    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            CompFull(0.0),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();
    let client_entity = fixtures::replicate_to_client(&mut stepper, server_entity);
    stepper.step(SETTLE, stepper.frame_interval(), Order::ServerFirst);
    AUDIT.budget(LINK + 1).run(|| {
        let server = value(stepper.server_app_mut(), server_entity);
        let client = value(stepper.client_app_mut(0), client_entity);
        stepper.step(1, stepper.frame_interval(), Order::ServerFirst);
        assert_eq!(value(stepper.server_app_mut(), server_entity), server + 1.0);
        assert_eq!(
            value(stepper.client_app_mut(0), client_entity),
            client + 1.0
        );
    });
    let history = stepper
        .client_app(0)
        .world()
        .get::<PredictionHistory<CompFull>>(client_entity)
        .unwrap();
    assert!(!history.is_empty(), "the client records predicted values");
}
