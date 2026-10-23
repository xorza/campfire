use std::sync::Barrier;
use std::thread;

use crate::thread_name::ThreadName;

/// A test's race: copies of one closure, each on a thread of its own, released at once by a
/// barrier, so a test meets what two writers of one file do at the same time.
#[derive(Debug)]
pub struct Race;

impl Race {
    /// The name of each of its threads, as a profiler shows it.
    const NAME: ThreadName = ThreadName::fixture("race");

    /// What `run` gave on each of `count` threads, by the index each was given.
    pub fn run<T: Send>(count: usize, run: impl Fn(usize) -> T + Sync) -> Vec<T> {
        Self::race(count, &Barrier::new(count), &run)
    }

    /// `run`, its racers released by `barrier`; each thread's closure moves the references in.
    fn race<T: Send>(count: usize, barrier: &Barrier, run: &(dyn Fn(usize) -> T + Sync)) -> Vec<T> {
        thread::scope(|scope| {
            #[expect(
                clippy::needless_collect,
                reason = "every racer spawns before the first join, or the barrier waits forever"
            )]
            let threads: Vec<_> = (0..count)
                .map(|index| {
                    let builder = thread::Builder::new().name(Self::NAME.as_str().to_owned());
                    let started = builder.spawn_scoped(scope, move || {
                        barrier.wait();
                        run(index)
                    });
                    started.expect("a racer's thread starts")
                })
                .collect();
            threads
                .into_iter()
                .map(|thread| thread.join().expect("a racer does not panic"))
                .collect()
        })
    }
}
