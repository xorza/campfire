use std::sync::Barrier;
use std::thread;

/// A test's race: copies of one closure, each on a thread of its own, released at once by a
/// barrier, so a test meets what two writers of one file do at the same time.
#[derive(Debug)]
pub struct Race;

impl Race {
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
                    scope.spawn(move || {
                        barrier.wait();
                        run(index)
                    })
                })
                .collect();
            threads
                .into_iter()
                .map(|thread| thread.join().expect("a racer does not panic"))
                .collect()
        })
    }
}
