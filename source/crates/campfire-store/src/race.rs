use std::sync::Barrier;
use std::thread;

/// A test's race: copies of one closure, each on a thread of its own, released at once by a
/// barrier, so a test meets what two writers of one file do at the same time.
#[derive(Debug)]
pub struct Race;

impl Race {
    /// What `run` gave on each of `count` threads, by the index each was given.
    pub fn run<T: Send>(count: usize, run: impl Fn(usize) -> T + Sync) -> Vec<T> {
        let barrier = Barrier::new(count);
        thread::scope(|scope| {
            let threads: Vec<_> = (0..count)
                .map(|index| {
                    let (barrier, run) = (&barrier, &run);
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
