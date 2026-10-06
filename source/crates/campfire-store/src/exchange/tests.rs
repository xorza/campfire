use super::*;

#[test]
fn each_job_comes_back_with_its_answer_and_its_buffer_is_reused() {
    // The worker sums the job's numbers; the caller refills the buffer it gave back.
    let mut exchange: Exchange<Vec<u32>, u32> =
        Exchange::start("sums", |job: &mut Vec<u32>| job.iter().sum());
    assert!(!exchange.pending());
    assert_eq!(exchange.take(), None);
    assert_eq!(exchange.wait(), None);
    exchange.send(|job| job.extend([1, 2, 3]));
    assert!(exchange.pending());
    assert_eq!(exchange.wait(), Some(6));
    assert!(!exchange.pending());
    let buffer = exchange.spare.as_ptr();
    let capacity = exchange.spare.capacity();
    exchange.send(|job| {
        assert_eq!(*job, [1, 2, 3]);
        job.clear();
        job.push(10);
    });
    let answer = loop {
        if let Some(answer) = exchange.take() {
            break answer;
        }
    };
    assert_eq!(answer, 10);
    assert_eq!(
        (exchange.spare.as_ptr(), exchange.spare.capacity()),
        (buffer, capacity)
    );
}

#[test]
#[should_panic(expected = "one job at a time")]
fn a_second_job_before_the_first_answer_is_refused() {
    let mut exchange: Exchange<u32, u32> = Exchange::start("one", |job: &mut u32| *job);
    exchange.send(|job| *job = 1);
    exchange.send(|job| *job = 2);
}
