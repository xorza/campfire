use campfire_common::Ticks;

/// The limits on a session's inputs: how many ticks an input may come late or early, its
/// largest payload, and the most a player sends a tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputRules {
    pub max_input_delay: Ticks,
    pub max_input_lead: Ticks,
    pub max_payload_len: u32,
    pub max_inputs_per_tick: u32,
}

impl InputRules {
    /// The limits the reference server runs a session on a LAN by.
    pub const LAN: InputRules = InputRules {
        max_input_delay: Ticks::new(10),
        max_input_lead: Ticks::new(30),
        max_payload_len: 64,
        max_inputs_per_tick: 4,
    };

    /// Limits no test reaches.
    #[cfg(feature = "internals")]
    pub const ROOMY: InputRules = InputRules {
        max_input_delay: Ticks::new(10),
        max_input_lead: Ticks::new(10),
        max_payload_len: 256,
        max_inputs_per_tick: 4,
    };
}
