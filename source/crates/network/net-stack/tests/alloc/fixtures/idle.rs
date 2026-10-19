use crate::fixtures::AUDIT;
use crate::fixtures::LINK;
use campfire_net_stack::internals::FullStack;
use campfire_net_stepper::ClientKind;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperConfig;

#[track_caller]
fn audit_idle(kind: ClientKind) {
    let mut stepper = Stepper::new(FullStack, StepperConfig::with_clients(vec![kind]));
    AUDIT.budget(LINK).run(|| stepper.frame_step(1));
}

#[test]
fn idle_link() {
    audit_idle(ClientKind::Raw);
}

/// The same in a sealed session of the UDP protocol, whose seal and open work in place.
#[test]
fn idle_sealed_link() {
    audit_idle(ClientKind::Udp);
}
