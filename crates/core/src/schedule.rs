//! Which periodic jobs are due on each beat of one timer. No toolkit in it, so it is unit tested.
//!
//! One timer ticks at the base interval; each job runs every so many milliseconds (rounded up to a whole number
//! of ticks). This replaces a timer per job and makes the cadence of every background task visible in one place.

#[derive(Debug)]
pub struct Schedule<T> {
    base_ms: u32,
    /// (job, every this many ticks)
    jobs: Vec<(T, u64)>,
}

impl<T: Copy + PartialEq> Schedule<T> {
    /// A schedule whose timer ticks every `base_ms` milliseconds.
    pub fn new(base_ms: u32) -> Self {
        Schedule { base_ms: base_ms.max(1), jobs: Vec::new() }
    }

    pub fn base_ms(&self) -> u32 {
        self.base_ms
    }

    /// Run `job` every `every_ms` milliseconds (at least every tick).
    pub fn every(mut self, job: T, every_ms: u32) -> Self {
        let ticks = every_ms.div_ceil(self.base_ms).max(1) as u64;
        self.jobs.push((job, ticks));
        self
    }

    /// The jobs to run on tick number `tick` (counting from 1), in the order they were added.
    pub fn due(&self, tick: u64) -> Vec<T> {
        self.jobs.iter().filter(|(_, n)| tick > 0 && tick % n == 0).map(|(j, _)| *j).collect()
    }

    /// Every ticks-per-run, for a given job.
    pub fn ticks_for(&self, job: T) -> Option<u64> {
        self.jobs.iter().find(|(j, _)| *j == job).map(|(_, n)| *n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Job {
        Fast,
        Medium,
        Slow,
    }

    fn schedule() -> Schedule<Job> {
        Schedule::new(500).every(Job::Fast, 500).every(Job::Medium, 1000).every(Job::Slow, 10_000)
    }

    #[test]
    fn a_job_every_tick_runs_on_every_tick() {
        let s = schedule();
        assert!((1..=40).all(|t| s.due(t).contains(&Job::Fast)));
    }

    #[test]
    fn a_slower_job_runs_on_multiples_of_its_period() {
        let s = schedule();
        let medium: Vec<u64> = (1..=6).filter(|t| s.due(*t).contains(&Job::Medium)).collect();
        assert_eq!(medium, vec![2, 4, 6]);
        let slow: Vec<u64> = (1..=40).filter(|t| s.due(*t).contains(&Job::Slow)).collect();
        assert_eq!(slow, vec![20, 40]);
    }

    #[test]
    fn nothing_runs_on_tick_zero() {
        assert!(schedule().due(0).is_empty());
    }

    #[test]
    fn jobs_come_back_in_the_order_added() {
        assert_eq!(schedule().due(20), vec![Job::Fast, Job::Medium, Job::Slow]);
    }

    #[test]
    fn periods_round_up_to_whole_ticks_and_never_below_one() {
        let s = Schedule::new(500).every(Job::Fast, 1).every(Job::Medium, 600);
        assert_eq!(s.ticks_for(Job::Fast), Some(1));
        assert_eq!(s.ticks_for(Job::Medium), Some(2));
    }

    #[test]
    fn an_unknown_job_has_no_period() {
        let s: Schedule<Job> = Schedule::new(500);
        assert_eq!(s.ticks_for(Job::Slow), None);
        assert_eq!(s.base_ms(), 500);
    }
}
