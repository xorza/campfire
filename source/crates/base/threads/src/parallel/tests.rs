use std::sync::{Barrier, Mutex};

use super::*;

/// Each index runs once and its result comes back in its place; no indices give nothing.
#[test]
fn each_index_runs_once_in_its_place() {
    assert_eq!(Parallel::map(0, |index| index), Vec::<usize>::new());
    assert_eq!(Parallel::map(1, |index| index * 10), [0]);
    let runs = Mutex::new(Vec::new());
    let squares = Parallel::map(100, |index| {
        runs.lock().unwrap().push(index);
        index * index
    });
    assert_eq!(
        squares,
        (0..100).map(|index| index * index).collect::<Vec<_>>()
    );
    let mut runs = runs.into_inner().unwrap();
    runs.sort_unstable();
    assert_eq!(runs, (0..100).collect::<Vec<_>>());
}

/// As many jobs as cores each wait until all of them run, so no thread takes a second job:
/// they run on one thread of their own for each core, none the caller's. A pool narrower
/// than the cores never releases the barrier, and the test ends at its bound.
#[test]
fn the_jobs_run_on_one_thread_for_each_core() {
    let cores = thread::available_parallelism().map_or(1, NonZero::get);
    let barrier = Barrier::new(cores);
    let threads = Parallel::map(cores, |_| {
        barrier.wait();
        thread::current().id()
    });
    assert!(!threads.contains(&thread::current().id()));
    for (index, id) in threads.iter().enumerate() {
        assert!(
            !threads[..index].contains(id),
            "job {index} shares a thread"
        );
    }
}

/// Each thread makes its state once, at its first index, and each index meets the state of
/// the thread that ran it: as many states as threads that took an index, never more.
#[test]
fn each_thread_makes_its_state_once() {
    let made = AtomicUsize::new(0);
    let cores = thread::available_parallelism().map_or(1, NonZero::get);
    let seen = Parallel::map_init(
        100,
        || (made.fetch_add(1, Ordering::Relaxed), Vec::new()),
        |(state, indices), index| {
            indices.push(index);
            (*state, indices.len())
        },
    );
    let states = made.into_inner();
    assert!((1..=cores.min(100)).contains(&states), "{states} states");
    // Each state's indices count up from 1, so each index met its thread's one state.
    let mut counts = vec![0; states];
    for (state, count) in seen {
        counts[state] += 1;
        assert_eq!(count, counts[state]);
    }
    assert_eq!(counts.iter().sum::<usize>(), 100);
}

#[test]
#[should_panic(expected = "job 3")]
fn a_jobs_panic_passes_on() {
    Parallel::map(8, |index| assert_ne!(index, 3, "job 3"));
}
