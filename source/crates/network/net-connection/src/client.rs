use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;

/// Marker component to identify this entity as a Client
#[derive(Debug, Component, Default, Reflect)]
pub struct Client;
