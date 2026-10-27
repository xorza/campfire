use std::f32::consts::{PI, TAU};

use bevy::math::{Affine3A, Vec2, Vec3};
use campfire_client_files::{
    AlphaKey, Blend, ColorKey, EmissionVelocity, EmissionVolume, ParticleSystem, Spread,
};
use campfire_math::SplitMix64;

use crate::view::effects::effect_book::{EffectBook, GroupId, ListId, SystemId};

/// The particle systems alive and their particles, stepped in whole steps of the book's length
/// (02-engine-core.md, A system steps as its file says; 02-engine-core.md, A cap by priority, from
/// the file), as Zero Hour's `ParticleSystemManager` updates its systems each logic frame
/// (`ParticleSystem::update`, `Particle::update` and `Particle::draw` at a frame's time scale of 1,
/// as GeneralsX gives them). Each slot of a system or a particle is reused once its own goes; a
/// step allocates nothing once the slots grew to the most alive at once.
#[derive(Debug)]
pub(crate) struct ParticleRun {
    rng: SplitMix64,
    systems: Vec<SystemSlot>,
    /// The systems alive, in the order they started, which is the order they step in.
    order: Vec<u32>,
    free_systems: Vec<u32>,
    particles: Vec<Particle>,
    free_particles: Vec<u32>,
    alive: u32,
    /// The systems the last effect played started on its object, which follow it.
    attached: Vec<SystemKey>,
    /// The particles made, which orders them by age.
    made: u64,
    /// The steps run.
    now: u64,
}

/// A started system, by its slot and the slot's generation, which a later system of the slot
/// does not answer to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SystemKey {
    slot: u32,
    generation: u32,
}

/// A slot of a system: its generation and the system in it, none once it went.
#[derive(Debug)]
struct SystemSlot {
    generation: u32,
    system: Option<LiveSystem>,
}

/// A system alive: its book's id, where it stands and how it turns, its place at its last step,
/// for a moving system's particles of a burst to spread along its move, whether it rides a
/// particle or follows its object, the steps left of its
/// delay, of its burst's and of its life, the system it emits into, that emits into it, and the
/// particle it rides, how much a burst's sizes grew, how many particles it holds, whether it was
/// stopped, and the scales of its particles' velocity, on its own axes, and of its bursts' counts.
#[derive(Debug)]
struct LiveSystem {
    id: SystemId,
    placement: Affine3A,
    last: Option<Vec3>,
    delay_left: u32,
    /// Whether it spent the step on its delay, so neither its particles nor its life count it.
    waiting: bool,
    burst_left: u32,
    life_left: u32,
    forever: bool,
    destroyed: bool,
    master: Option<u32>,
    slave: Option<u32>,
    rider: Option<u32>,
    size_bonus: f32,
    particles: u32,
    stopped: bool,
    velocity_scale: Vec3,
    count_scale: f32,
}

/// A particle: the system it is of, the order it was made in, the step it was made at, its rank
/// under the cap, the system it carries, and its state, each value drawn once as it is made.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Particle {
    alive: bool,
    system: u32,
    made: u64,
    born: u64,
    rank: Option<u8>,
    carries: Option<u32>,
    pub(crate) position: Vec3,
    velocity: Vec3,
    velocity_damping: f32,
    pub(crate) angle: f32,
    angular_rate: f32,
    angular_damping: f32,
    pub(crate) size: f32,
    size_rate: f32,
    size_rate_damping: f32,
    life_left: u32,
    pub(crate) alpha: f32,
    alpha_rate: f32,
    alpha_target: usize,
    alpha_target_value: f32,
    pub(crate) color: Vec3,
    color_rate: Vec3,
    color_target: usize,
    color_scale: f32,
    emitter: Vec3,
    up_towards_emitter: bool,
    pub(crate) ground_aligned: bool,
    pub(crate) group: GroupId,
}

/// What a new particle draws of its system: its place and its velocity, in the world, and the
/// rest of its values (`ParticleInfo`).
#[derive(Debug, Clone, Copy)]
struct ParticleStart {
    position: Vec3,
    velocity: Vec3,
    velocity_damping: f32,
    angle: f32,
    angular_rate: f32,
    angular_damping: f32,
    lifetime: u32,
    size: f32,
    size_rate: f32,
    size_rate_damping: f32,
    color_scale: f32,
    emitter: Vec3,
}

/// The most a burst's sizes grow by its systems' `start_size_rate` (`MAX_SIZE_BONUS`).
const MOST_SIZE_BONUS: f32 = 50.0;
/// The alpha below which, and the shares of a color past which, a particle past its last key
/// draws nothing, and goes (`Particle::isInvisible`).
const UNSEEN: f32 = 0.01;

impl ParticleRun {
    /// A run of no system, drawing from `seed`.
    pub(crate) const fn new(seed: u64) -> Self {
        Self {
            rng: SplitMix64::new(seed),
            systems: Vec::new(),
            order: Vec::new(),
            free_systems: Vec::new(),
            particles: Vec::new(),
            free_particles: Vec::new(),
            alive: 0,
            attached: Vec::new(),
            made: 0,
            now: 0,
        }
    }

    /// Plays one of `lists`, drawn at random, as `play` plays it; none of no list.
    pub(crate) fn play_one(
        &mut self,
        book: &EffectBook,
        lists: &[ListId],
        source: Affine3A,
        ground: impl Fn(f32, f32) -> f32,
    ) {
        let count = u64::try_from(lists.len()).expect("a count fits u64");
        if count == 0 {
            return;
        }
        let at = usize::try_from(self.below(count)).expect("an index below the count");
        self.play(book, lists[at], source, ground);
    }

    /// Plays the list `list` of `book` from `source`, the place and the turn of what plays it
    /// (`ParticleSystemFXNugget::reallyDoFX`): each entry's system as many times as its count,
    /// each at the source's place, plus its offset turned as the source, plus its radius at an
    /// angle drawn about the vertical, at its height above, or on the ground, which `ground`
    /// gives; turned by its rotation, after the source's turn when it orients to it; its delay,
    /// rounded up, in place of the system's own, when it has one and draws one not below 0.
    /// The systems that follow their object are its `attached` until the next play.
    pub(crate) fn play(
        &mut self,
        book: &EffectBook,
        list: ListId,
        source: Affine3A,
        ground: impl Fn(f32, f32) -> f32,
    ) {
        self.attached.clear();
        let (_, turn, place) = source.to_scale_rotation_translation();
        for entry in book.list(list) {
            let spec = &entry.spec;
            let offset = turn * spec.offset;
            for _ in 0..spec.count {
                let radius = self.draw(spec.radius);
                let angle = self.uniform(0.0, TAU);
                let mut at =
                    place + offset + Vec3::new(radius * angle.cos(), 0.0, radius * angle.sin());
                if spec.at_ground {
                    at.y = ground(at.x, at.z);
                } else {
                    at.y += self.draw(spec.height);
                }
                let oriented = if spec.orient_to_object {
                    turn * spec.rotation
                } else {
                    spec.rotation
                };
                let placement = Affine3A::from_rotation_translation(oriented, at);
                let key = self.start(book, entry.system, placement);
                if let Some(delay) = spec.delay {
                    let drawn = self.draw(delay);
                    if drawn >= 0.0 {
                        self.system_mut(key.slot).delay_left = Self::whole(drawn.ceil());
                    }
                }
                if spec.attach_to_object {
                    self.attached.push(key);
                }
            }
        }
    }

    /// The systems the last play started on its object.
    pub(crate) fn attached(&self) -> &[SystemKey] {
        &self.attached
    }

    /// Starts the system `id` of `book` at `placement`, with its slave, as
    /// `ParticleSystemManager::createParticleSystem` makes one; the slave's own slave, which the
    /// game makes too, takes no particle, as a slave emits none, so it is not made.
    pub(crate) fn start(
        &mut self,
        book: &EffectBook,
        id: SystemId,
        placement: Affine3A,
    ) -> SystemKey {
        let slot = self.make_system(book, id, placement);
        if let Some(slave) = book.system(id).slave {
            let slave = self.make_system(book, slave, placement);
            self.system_mut(slave).master = Some(slot);
            self.system_mut(slot).slave = Some(slave);
        }
        self.key(slot)
    }

    /// Moves the system of `key`, if it is still alive, to `placement`, as a system on an
    /// object follows it.
    pub(crate) fn place(&mut self, key: SystemKey, placement: Affine3A) {
        if let Some(system) = self.live_mut(key) {
            system.placement = placement;
        }
    }

    /// Stops the system of `key` from emitting, and its slave, if it is still alive; its
    /// particles live out their lives (`ParticleSystem::destroy`).
    pub(crate) fn destroy(&mut self, key: SystemKey) {
        if self.live_mut(key).is_some() {
            self.destroy_slot(key.slot);
        }
    }

    /// Stops the system of `key` from emitting until it resumes, if it is still alive; it keeps
    /// its particles and its life (`ParticleSystem::stop`).
    pub(crate) fn stop(&mut self, key: SystemKey) {
        if let Some(system) = self.live_mut(key) {
            system.stopped = true;
        }
    }

    /// Lets the system of `key` emit again, if it is still alive (`ParticleSystem::start`).
    pub(crate) fn resume(&mut self, key: SystemKey) {
        if let Some(system) = self.live_mut(key) {
            system.stopped = false;
        }
    }

    /// Scales the velocity of the particles the system of `key` makes by `velocity`, on its own
    /// axes, and its bursts' counts by `count`, if it is still alive (`setVelocityMultiplier`,
    /// `setBurstCountMultiplier`).
    pub(crate) fn scale(&mut self, key: SystemKey, velocity: Vec3, count: f32) {
        if let Some(system) = self.live_mut(key) {
            system.velocity_scale = velocity;
            system.count_scale = count;
        }
    }

    /// The particles alive, in no order.
    pub(crate) fn particles(&self) -> impl Iterator<Item = &Particle> {
        self.particles.iter().filter(|particle| particle.alive)
    }

    /// Steps every system once, in the order they started: each emits its burst, then each
    /// particle moves, ages and fades, then each system counts its life; `ground` gives the
    /// ground's height under a point's `x` and `z`, which a system that emits above the ground
    /// alone tests.
    pub(crate) fn step(&mut self, book: &EffectBook, ground: impl Fn(f32, f32) -> f32) {
        let mut at = 0;
        while at < self.order.len() {
            let slot = self.order[at];
            self.emit(book, slot, &ground);
            at += 1;
        }
        for slot in 0..self.particles.len() {
            if self.particles[slot].alive {
                let index = u32::try_from(slot).expect("a run's particles fit u32");
                self.update_particle(book, index);
            }
        }
        let mut kept = 0;
        for at in 0..self.order.len() {
            let slot = self.order[at];
            if self.count_life(slot) {
                self.order[kept] = slot;
                kept += 1;
            } else {
                self.remove_system(slot);
            }
        }
        self.order.truncate(kept);
        self.now += 1;
    }

    /// The system of `slot` emits its burst, as its delays and its life allow, after it follows
    /// the particle it rides, and keeps where it stood for the next step's burst.
    fn emit(&mut self, book: &EffectBook, slot: u32, ground: &impl Fn(f32, f32) -> f32) {
        let system = self.system_mut(slot);
        system.waiting = system.delay_left > 0;
        if system.waiting {
            system.delay_left -= 1;
            return;
        }
        if let Some(rider) = system.rider {
            let position = self.particles[Self::index(rider)].position;
            self.system_mut(slot).placement.translation = position.into();
        }
        self.burst(book, slot, ground);
        let system = self.system_mut(slot);
        system.last = Some(system.placement.translation.into());
    }

    /// The system of `slot` emits its burst, as its burst's delay and its life allow.
    fn burst(&mut self, book: &EffectBook, slot: u32, ground: &impl Fn(f32, f32) -> f32) {
        let system = self.system(slot);
        let spec = &book.system(system.id).spec;
        let emits = !system.destroyed
            && !system.stopped
            && (system.forever || system.life_left > 0)
            && system.master.is_none();
        if !emits {
            return;
        }
        let count_scale = system.count_scale;
        if system.burst_left > 0 {
            self.system_mut(slot).burst_left -= 1;
            return;
        }
        let count = Self::whole(self.draw(spec.burst_count));
        // The game truncates the drawn count, then its product by the scale.
        #[expect(
            clippy::cast_precision_loss,
            reason = "a burst's count lies far below 2²⁴"
        )]
        let count = Self::whole(count as f32 * count_scale);
        let rank = spec.rank;
        let above_ground_only = spec.above_ground_only;
        for number in 0..count {
            let start = self.particle_start(book, slot, number, count);
            if above_ground_only && start.position.y < ground(start.position.x, start.position.z) {
                continue;
            }
            let Some(particle) = self.make_particle(book, slot, &start, rank) else {
                continue;
            };
            let system = self.system(slot);
            if let Some(attached) = book.system(system.id).attached {
                let placement = Affine3A::from_translation(start.position);
                let carried = self.start(book, attached, placement).slot;
                self.system_mut(carried).rider = Some(particle);
                self.particles[Self::index(particle)].carries = Some(carried);
            }
            if let Some(slave) = self.system(slot).slave {
                let start = self.slave_start(book, slot, slave);
                self.make_particle(book, slave, &start, rank);
            }
        }
        let delay = Self::whole(self.draw(book.system(self.system(slot).id).spec.burst_delay));
        self.system_mut(slot).burst_left = delay;
    }

    /// A new particle's values of the system of `slot`, the `number`th of a burst of `count`
    /// (`generateParticleInfo`): its place in the volume and its velocity, turned and moved
    /// as the system stands, less the share of the system's move since its last step the
    /// particles after it in the burst have yet to make.
    fn particle_start(
        &mut self,
        book: &EffectBook,
        slot: u32,
        number: u32,
        count: u32,
    ) -> ParticleStart {
        let spec = &book.system(self.system(slot).id).spec;
        let local = self.volume_point(spec);
        let velocity = self.velocity(spec, local) * self.system(slot).velocity_scale;
        let system = self.system(slot);
        let placement = system.placement;
        let now = Vec3::from(placement.translation);
        let moved = system.last.map_or(Vec3::ZERO, |last| now - last);
        #[expect(
            clippy::cast_precision_loss,
            reason = "a burst's count lies far below 2²⁴"
        )]
        let behind = 1.0 - number as f32 / count as f32;
        let mut start = ParticleStart {
            position: placement.transform_point3(local) - moved * behind,
            velocity: placement.transform_vector3(velocity),
            velocity_damping: self.draw(spec.velocity_damping),
            angle: self.draw(spec.angle),
            angular_rate: self.draw(spec.angular_rate),
            angular_damping: self.draw(spec.angular_damping),
            lifetime: Self::whole(self.draw(spec.lifetime)),
            size: self.draw(spec.size),
            size_rate: self.draw(spec.size_rate),
            size_rate_damping: self.draw(spec.size_rate_damping),
            color_scale: self.draw(spec.color_scale),
            emitter: now,
        };
        let bonus = self.system(slot).size_bonus;
        start.size += bonus;
        let grown = bonus + self.draw(spec.start_size_rate);
        self.system_mut(slot).size_bonus = if grown == 0.0 {
            grown
        } else {
            grown.min(MOST_SIZE_BONUS)
        };
        start
    }

    /// The values of a particle the system of `slot` emits into its slave, of `slave`
    /// (`mergeRelatedParticleSystems`): a new particle's of the system, its life, its turn and
    /// its keys the slave's, its sizes those of both, and its place moved by the slave's offset.
    fn slave_start(&mut self, book: &EffectBook, slot: u32, slave: u32) -> ParticleStart {
        let mut start = self.particle_start(book, slot, 1, 1);
        let own = self.particle_start(book, slave, 1, 1);
        start.lifetime = own.lifetime;
        start.size *= own.size;
        start.size_rate *= own.size_rate;
        start.size_rate_damping *= own.size_rate_damping;
        start.angle = own.angle;
        start.angular_rate = own.angular_rate;
        start.angular_damping = own.angular_damping;
        start.color_scale = own.color_scale;
        let spec = &book.system(self.system(slot).id).spec;
        start.position += spec.slave.as_ref().map_or(Vec3::ZERO, |slave| slave.offset);
        start
    }

    /// Makes a particle of `start` in the system of `slot`, of `rank` under the cap: past the
    /// cap, the oldest particles of the lowest ranks below it go first, and none is made when
    /// too few of them are alive (`ParticleSystem::createParticle`).
    fn make_particle(
        &mut self,
        book: &EffectBook,
        slot: u32,
        start: &ParticleStart,
        rank: Option<u8>,
    ) -> Option<u32> {
        if let Some(rank) = rank {
            let cap = book.cap();
            let excess = self.alive.saturating_sub(cap);
            for _ in 0..excess {
                let below = self.particles.iter().enumerate().filter(|(_, particle)| {
                    particle.alive && particle.rank.is_some_and(|own| own < rank)
                });
                let oldest = below.min_by_key(|(_, particle)| (particle.rank, particle.made));
                let (index, _) = oldest?;
                self.kill(u32::try_from(index).expect("a run's particles fit u32"));
            }
            if cap == 0 {
                return None;
            }
        }
        let system = self.system(slot);
        let book_system = book.system(system.id);
        let spec = &book_system.spec;
        let [alpha, next] = [0, 1].map(|at| spec.alpha.get(at).map(|key| key.alpha));
        let alpha = self.draw(alpha.expect("a system has an alpha key"));
        let alpha_target_value = next.map_or(alpha, |next| self.draw(next));
        let color = Vec3::from(spec.color[0].color);
        let particle = Particle {
            alive: true,
            system: slot,
            made: self.made,
            born: self.now,
            rank,
            carries: None,
            position: start.position,
            velocity: start.velocity,
            velocity_damping: start.velocity_damping,
            angle: start.angle,
            angular_rate: start.angular_rate,
            angular_damping: start.angular_damping,
            size: start.size,
            size_rate: start.size_rate,
            size_rate_damping: start.size_rate_damping,
            life_left: start.lifetime,
            alpha,
            alpha_rate: Self::alpha_rate(&spec.alpha, 1, alpha, alpha_target_value),
            alpha_target: 1,
            alpha_target_value,
            color,
            color_rate: Self::color_rate(&spec.color, 1),
            color_target: 1,
            color_scale: start.color_scale,
            emitter: start.emitter,
            up_towards_emitter: spec.up_towards_emitter,
            ground_aligned: spec.ground_aligned,
            group: book_system.group,
        };
        self.made += 1;
        self.alive += 1;
        self.system_mut(slot).particles += 1;
        let index = if let Some(free) = self.free_particles.pop() {
            self.particles[Self::index(free)] = particle;
            free
        } else {
            self.particles.push(particle);
            u32::try_from(self.particles.len() - 1).expect("a run's particles fit u32")
        };
        Some(index)
    }

    /// Steps the particle of `index` (`Particle::update`, then `Particle::draw` at a time scale
    /// of 1): its life counts down, its velocity takes its system's gravity and its damping;
    /// past the step it was made in, its rates are damped and its keys move on; then it moves,
    /// turns, grows, fades and tints. It goes at the end of its life, or past its last key once
    /// it draws nothing.
    fn update_particle(&mut self, book: &EffectBook, index: u32) {
        let now = self.now;
        let particle = &self.particles[Self::index(index)];
        let system = self.systems[Self::index(particle.system)]
            .system
            .as_ref()
            .expect("a particle's system is alive");
        if system.waiting {
            return;
        }
        let spec = &book.system(system.id).spec;
        let particle = &mut self.particles[Self::index(index)];
        if particle.life_left > 0 {
            particle.life_left -= 1;
            if particle.life_left == 0 {
                self.kill(index);
                return;
            }
        }
        particle.velocity.y += spec.gravity;
        particle.velocity *= particle.velocity_damping;
        let age = now - particle.born;
        if age > 0 {
            particle.angular_rate *= particle.angular_damping;
            particle.size_rate *= particle.size_rate_damping;
            if spec.blend != Blend::Add {
                self.move_alpha(spec, index, age);
            }
            let particle = &mut self.particles[Self::index(index)];
            let target = particle.color_target;
            if target < spec.color.len() {
                if age > u64::from(spec.color[target].at) {
                    particle.color_target += 1;
                    particle.color_rate = Self::color_rate(&spec.color, particle.color_target);
                }
            } else {
                particle.color_rate = Vec3::ZERO;
            }
            if Self::unseen(particle, spec) {
                self.kill(index);
                return;
            }
        }
        let particle = &mut self.particles[Self::index(index)];
        particle.position += particle.velocity + spec.drift;
        particle.angle += particle.angular_rate;
        if particle.up_towards_emitter {
            // The game's ground plane is the engine's `x` and `−z`.
            let away = Vec2::new(
                particle.position.x - particle.emitter.x,
                particle.emitter.z - particle.position.z,
            );
            particle.angle = Self::angle_from_up(away) + PI;
        }
        particle.size += particle.size_rate;
        if spec.blend != Blend::Add {
            particle.alpha = (particle.alpha + particle.alpha_rate).clamp(0.0, 1.0);
        }
        particle.color = (particle.color + particle.color_rate + Vec3::splat(particle.color_scale))
            .clamp(Vec3::ZERO, Vec3::ONE);
    }

    /// Moves the alpha of the particle of `index`, `age` steps old, on to its next key once it
    /// passed its target's step, and lands it on its second key at that key's step.
    fn move_alpha(&mut self, spec: &ParticleSystem, index: u32, age: u64) {
        let particle = &mut self.particles[Self::index(index)];
        if particle.alpha_target == 1
            && spec
                .alpha
                .get(1)
                .is_some_and(|key| age == u64::from(key.at))
        {
            particle.alpha = particle.alpha_target_value;
            particle.alpha_rate = 0.0;
        }
        let target = particle.alpha_target;
        if target >= spec.alpha.len() {
            particle.alpha_rate = 0.0;
            return;
        }
        if age > u64::from(spec.alpha[target].at) {
            let from = particle.alpha_target_value;
            let next = spec.alpha.get(target + 1).map(|key| key.alpha);
            let to = next.map(|next| self.draw(next));
            let particle = &mut self.particles[Self::index(index)];
            particle.alpha_target += 1;
            particle.alpha_target_value = to.unwrap_or(from);
            particle.alpha_rate = Self::alpha_rate(
                &spec.alpha,
                particle.alpha_target,
                from,
                particle.alpha_target_value,
            );
        }
    }

    /// The rate from the key before `target` to `target`, of the values `from` and `to`, over
    /// their steps; 0 past the last key (`computeAlphaRate`).
    #[expect(
        clippy::cast_precision_loss,
        reason = "a key's step lies far below 2²⁴"
    )]
    fn alpha_rate(keys: &[AlphaKey], target: usize, from: f32, to: f32) -> f32 {
        match keys.get(target) {
            Some(key) => (to - from) / (key.at - keys[target - 1].at) as f32,
            None => 0.0,
        }
    }

    /// The rate of each share of the color from the key before `target` to `target`; 0 past
    /// the last key (`computeColorRate`).
    #[expect(
        clippy::cast_precision_loss,
        reason = "a key's step lies far below 2²⁴"
    )]
    fn color_rate(keys: &[ColorKey], target: usize) -> Vec3 {
        match keys.get(target) {
            Some(key) => {
                let before = &keys[target - 1];
                (Vec3::from(key.color) - Vec3::from(before.color)) / (key.at - before.at) as f32
            }
            None => Vec3::ZERO,
        }
    }

    /// Whether `particle` draws nothing past its last key in its system's blend: added, of a
    /// color near black; mixed by its alpha, of an alpha near 0; multiplied, of a color near
    /// white (`Particle::isInvisible`).
    fn unseen(particle: &Particle, spec: &ParticleSystem) -> bool {
        let colors_done = particle.color_target >= spec.color.len();
        let alpha_done = particle.alpha_target >= spec.alpha.len();
        match spec.blend {
            Blend::Add | Blend::Screen => colors_done && particle.color.max_element() < UNSEEN,
            Blend::Alpha | Blend::AlphaMask { .. } => alpha_done && particle.alpha < UNSEEN,
            Blend::Multiply => colors_done && particle.color.min_element() > 1.0 - UNSEEN,
            Blend::Opaque | Blend::Mask { .. } => false,
        }
    }

    /// The turn from the ground's `y`, `(0, 1)`, to `away`, as the game's `angleBetween` gives
    /// it: its size, of the sign of `away`'s `x`, a negative one but where it lies to the right.
    fn angle_from_up(away: Vec2) -> f32 {
        let length = away.length();
        if length == 0.0 {
            return 0.0;
        }
        if away.y == 0.0 {
            return if away.x > 0.0 { PI } else { 0.0 };
        }
        let turn = (away.y / length).acos();
        if away.x > 0.0 { turn } else { -turn }
    }

    /// The system of `slot` counts its life once its particles stepped: whether it stays. One
    /// stopped stays while it holds particles, or while its slave, of a life of its own, lives;
    /// one of a life stays while it holds particles, and stops at the end of its life, or goes.
    fn count_life(&mut self, slot: u32) -> bool {
        let slave_lives = |run: &Self, system: &LiveSystem| {
            system.slave.is_some_and(|slave| !run.system(slave).forever)
        };
        let system = self.system(slot);
        if system.waiting {
            return true;
        }
        if system.destroyed && system.particles == 0 {
            return slave_lives(self, system);
        }
        if system.forever {
            return true;
        }
        let system = self.system_mut(slot);
        system.life_left = system.life_left.saturating_sub(1);
        if system.particles > 0 {
            return true;
        }
        if system.life_left == 0 {
            let system = self.system(slot);
            if slave_lives(self, system) {
                self.system_mut(slot).destroyed = true;
            } else {
                return false;
            }
        }
        true
    }

    /// The particle of `index` goes, and stops the system it carries.
    fn kill(&mut self, index: u32) {
        let particle = &mut self.particles[Self::index(index)];
        debug_assert!(particle.alive, "a particle goes once");
        particle.alive = false;
        let (system, carries) = (particle.system, particle.carries.take());
        self.system_mut(system).particles -= 1;
        self.alive -= 1;
        self.free_particles.push(index);
        if let Some(carried) = carries {
            self.system_mut(carried).rider = None;
            self.destroy_slot(carried);
        }
    }

    /// The system of `slot`, stopped, and its slave.
    fn destroy_slot(&mut self, slot: u32) {
        let system = self.system_mut(slot);
        system.destroyed = true;
        if let Some(slave) = system.slave {
            self.destroy_slot(slave);
        }
    }

    /// The system of `slot` goes, of no particle, its links to its master, its slave and the
    /// particle it rides cut.
    fn remove_system(&mut self, slot: u32) {
        let system = self.systems[Self::index(slot)]
            .system
            .take()
            .expect("a system goes once");
        debug_assert_eq!(system.particles, 0, "a system goes once its particles went");
        if let Some(slave) = system.slave {
            self.system_mut(slave).master = None;
        }
        if let Some(master) = system.master {
            self.system_mut(master).slave = None;
        }
        if let Some(rider) = system.rider {
            self.particles[Self::index(rider)].carries = None;
        }
        let entry = &mut self.systems[Self::index(slot)];
        entry.generation = entry.generation.wrapping_add(1);
        self.free_systems.push(slot);
    }

    /// Makes a system of `id` at `placement` in a free slot, its delay, its life and its burst
    /// drawn and counted from its book's.
    fn make_system(&mut self, book: &EffectBook, id: SystemId, placement: Affine3A) -> u32 {
        let spec = &book.system(id).spec;
        let delay_left = Self::whole(self.draw(spec.initial_delay));
        let system = LiveSystem {
            id,
            placement,
            last: None,
            delay_left,
            waiting: false,
            burst_left: 0,
            life_left: spec.system_lifetime,
            forever: spec.system_lifetime == 0,
            destroyed: false,
            master: None,
            slave: None,
            rider: None,
            size_bonus: 0.0,
            particles: 0,
            stopped: false,
            velocity_scale: Vec3::ONE,
            count_scale: 1.0,
        };
        let slot = if let Some(free) = self.free_systems.pop() {
            self.systems[Self::index(free)].system = Some(system);
            free
        } else {
            self.systems.push(SystemSlot {
                generation: 0,
                system: Some(system),
            });
            u32::try_from(self.systems.len() - 1).expect("a run's systems fit u32")
        };
        self.order.push(slot);
        slot
    }

    /// A point of `spec`'s emission volume, about its system's place
    /// (`computeParticlePosition`).
    fn volume_point(&mut self, spec: &ParticleSystem) -> Vec3 {
        match spec.volume {
            EmissionVolume::Point => Vec3::ZERO,
            EmissionVolume::Line { start, end } => start + (end - start) * self.uniform(0.0, 1.0),
            EmissionVolume::Box { half } => {
                if spec.hollow {
                    // A side of the six alike, where the game draws one of seven numbers for them
                    // and puts the side of `x` at `y`'s half size.
                    let side = self.below(6);
                    let axis = usize::try_from(side % 3).expect("an axis fits usize");
                    let mut point = Vec3::new(
                        self.uniform(-half.x, half.x),
                        self.uniform(-half.y, half.y),
                        self.uniform(-half.z, half.z),
                    );
                    point[axis] = if side < 3 { -half[axis] } else { half[axis] };
                    point
                } else {
                    Vec3::new(
                        self.uniform(-half.x, half.x),
                        self.uniform(-half.y, half.y),
                        self.uniform(-half.z, half.z),
                    )
                }
            }
            EmissionVolume::Sphere { radius } => {
                let radius = if spec.hollow {
                    radius
                } else {
                    self.uniform(0.0, radius)
                };
                self.on_sphere() * radius
            }
            EmissionVolume::Cylinder { radius, length } => {
                let angle = self.uniform(0.0, TAU);
                let radius = if spec.hollow {
                    radius
                } else {
                    self.uniform(0.0, radius)
                };
                let half = length / 2.0;
                Vec3::new(
                    radius * angle.cos(),
                    self.uniform(-half, half),
                    radius * angle.sin(),
                )
            }
        }
    }

    /// The velocity `spec` gives a particle starting at `point`, about its system's place
    /// (`computeParticleVelocity`).
    fn velocity(&mut self, spec: &ParticleSystem, point: Vec3) -> Vec3 {
        match spec.velocity {
            EmissionVelocity::Still => Vec3::ZERO,
            EmissionVelocity::Ortho { x, y, z } => {
                Vec3::new(self.draw(x), self.draw(y), self.draw(z))
            }
            EmissionVelocity::Cylindrical { radial, normal } => {
                let radial = self.draw(radial);
                let angle = self.uniform(0.0, TAU);
                Vec3::new(
                    radial * angle.cos(),
                    self.draw(normal),
                    radial * angle.sin(),
                )
            }
            EmissionVelocity::Spherical { speed } => self.on_sphere() * self.draw(speed),
            EmissionVelocity::Hemispherical { speed } => {
                let speed = self.draw(speed);
                loop {
                    let direction = Vec3::new(
                        self.uniform(-1.0, 1.0),
                        self.uniform(0.0, 1.0),
                        self.uniform(-1.0, 1.0),
                    );
                    if direction != Vec3::ZERO {
                        break direction.normalize() * speed;
                    }
                }
            }
            EmissionVelocity::Outward { speed, other } => {
                let speed = self.draw(speed);
                let other = self.draw(other);
                match spec.volume {
                    EmissionVolume::Cylinder { .. } => {
                        let out = Vec2::new(point.x, point.z).normalize_or_zero() * speed;
                        Vec3::new(out.x, other, out.y)
                    }
                    EmissionVolume::Box { .. } | EmissionVolume::Sphere { .. } => {
                        point.normalize_or_zero() * speed
                    }
                    EmissionVolume::Line { start, end } => {
                        let along = (end - start).normalize_or_zero();
                        let across = Vec3::Y.cross(along).normalize_or_zero();
                        let up = along.cross(across).normalize_or_zero();
                        across * speed + up * other
                    }
                    EmissionVolume::Point => self.on_sphere() * speed,
                }
            }
        }
    }

    /// A direction from the cube's points, as the game draws one on the sphere
    /// (`computePointOnUnitSphere`).
    fn on_sphere(&mut self) -> Vec3 {
        loop {
            let point = Vec3::new(
                self.uniform(-1.0, 1.0),
                self.uniform(-1.0, 1.0),
                self.uniform(-1.0, 1.0),
            );
            if point != Vec3::ZERO {
                return point.normalize();
            }
        }
    }

    /// A value of `spread`.
    fn draw(&mut self, spread: Spread) -> f32 {
        self.uniform(spread.low, spread.high)
    }

    /// A value from `low` to `high`, uniformly, from 24 bits of a draw.
    #[expect(clippy::cast_precision_loss, reason = "24 bits fit an f32 whole")]
    fn uniform(&mut self, low: f32, high: f32) -> f32 {
        let share = (self.rng.next_u64() >> 40) as f32 / (1_u32 << 24) as f32;
        low + (high - low) * share
    }

    /// A whole number below `bound`.
    fn below(&mut self, bound: u64) -> u64 {
        self.rng.below(bound)
    }

    /// `value` truncated to a count of no sign, as the game casts one.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the game truncates a drawn count; a negative one is none"
    )]
    const fn whole(value: f32) -> u32 {
        value.max(0.0) as u32
    }

    fn key(&self, slot: u32) -> SystemKey {
        SystemKey {
            slot,
            generation: self.systems[Self::index(slot)].generation,
        }
    }

    fn live_mut(&mut self, key: SystemKey) -> Option<&mut LiveSystem> {
        let entry = self.systems.get_mut(Self::index(key.slot))?;
        (entry.generation == key.generation)
            .then_some(entry.system.as_mut())
            .flatten()
    }

    fn system(&self, slot: u32) -> &LiveSystem {
        self.systems[Self::index(slot)]
            .system
            .as_ref()
            .expect("a slot's system is alive")
    }

    fn system_mut(&mut self, slot: u32) -> &mut LiveSystem {
        self.systems[Self::index(slot)]
            .system
            .as_mut()
            .expect("a slot's system is alive")
    }

    fn index(slot: u32) -> usize {
        usize::try_from(slot).expect("a u32 fits usize")
    }
}

#[cfg(test)]
pub(crate) mod samples {
    use std::collections::BTreeMap;

    use bevy::math::Vec3;
    use campfire_client_files::{
        AlphaKey, Blend, ColorKey, EffectsFile, EmissionVelocity, EmissionVolume, ParticleSystem,
        Spread,
    };
    use campfire_common::DeclaredName;
    use campfire_values::PackagePath;

    use crate::view::effects::effect_book::EffectBook;

    /// A system of one particle a burst, every step, each living 100 steps at rest, of size 1,
    /// opaque white, of no drift, damping or gravity.
    pub(crate) fn system() -> ParticleSystem {
        ParticleSystem {
            rank: Some(0),
            blend: Blend::Alpha,
            texture: PackagePath::parse("client/textures/spark.ktx2").unwrap(),
            ground_aligned: false,
            up_towards_emitter: false,
            above_ground_only: false,
            angle: Spread::ZERO,
            angular_rate: Spread::ZERO,
            angular_damping: Spread::of(1.0),
            velocity_damping: Spread::of(1.0),
            gravity: 0.0,
            drift: Vec3::ZERO,
            lifetime: Spread::of(100.0),
            system_lifetime: 0,
            size: Spread::of(1.0),
            start_size_rate: Spread::ZERO,
            size_rate: Spread::ZERO,
            size_rate_damping: Spread::of(1.0),
            alpha: vec![AlphaKey {
                alpha: Spread::of(1.0),
                at: 0,
            }],
            color: vec![ColorKey {
                color: [1.0; 3],
                at: 0,
            }],
            color_scale: Spread::ZERO,
            burst_delay: Spread::ZERO,
            burst_count: Spread::of(1.0),
            initial_delay: Spread::ZERO,
            velocity: EmissionVelocity::Still,
            volume: EmissionVolume::Point,
            hollow: false,
            slave: None,
            attached: None,
        }
    }

    pub(crate) fn name(text: &str) -> DeclaredName {
        DeclaredName::new(text).unwrap()
    }

    /// The book of `systems`, by their names, of a cap of `cap` particles.
    pub(crate) fn book(
        cap: u32,
        systems: impl IntoIterator<Item = (&'static str, ParticleSystem)>,
    ) -> EffectBook {
        let systems: BTreeMap<_, _> = systems
            .into_iter()
            .map(|(text, system)| (name(text), system))
            .collect();
        EffectBook::new(&EffectsFile::new(1.0 / 30.0, cap, systems, BTreeMap::new()).unwrap())
    }
}

#[cfg(test)]
mod tests;
