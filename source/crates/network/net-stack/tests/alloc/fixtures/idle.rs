use crate::fixtures::AUDIT;
use crate::fixtures::LINK;
use campfire_net_stack::internals::FullStack;
use campfire_net_stepper::ClientKind;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;

#[track_caller]
fn audit_idle(kind: ClientKind) {
    let mut stepper = Stepper::new(FullStack, StepperSetup::with_clients(kind, 1));
    AUDIT.budget(LINK).run(|| stepper.frame_step(1));
}

#[test]
fn an_idle_link_allocates_nothing() {
    audit_idle(ClientKind::Plain);
}

/// The same in a sealed packet session of the UDP protocol, whose seal and open work in place.
#[test]
fn an_idle_sealed_link_allocates_nothing() {
    audit_idle(ClientKind::Udp);
}
