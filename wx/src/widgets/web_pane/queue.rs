//! Holds scripts back until the page can run them. No toolkit in it, so it is unit tested.
//!
//! A page is not ready to be scripted until it has loaded (and, for some pages, announced itself). Scripts sent
//! before then are kept in order and released when it becomes ready; loading a new page makes it not ready again.

#[derive(Default, Debug)]
pub struct ScriptQueue {
    ready: bool,
    pending: Vec<String>,
}

impl ScriptQueue {
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Offer a script: it comes back to be run now when the page is ready, else it is kept.
    pub fn submit(&mut self, js: String) -> Option<String> {
        if self.ready {
            Some(js)
        } else {
            self.pending.push(js);
            None
        }
    }

    /// The page is ready: the kept scripts, in the order they were submitted.
    pub fn mark_ready(&mut self) -> Vec<String> {
        self.ready = true;
        std::mem::take(&mut self.pending)
    }

    /// A new page is loading. Kept scripts were meant for the old one, so they are dropped.
    pub fn reset(&mut self) {
        self.ready = false;
        self.pending.clear();
    }

    pub fn pending(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_wait_until_the_page_is_ready() {
        let mut q = ScriptQueue::default();
        assert_eq!(q.submit("a".into()), None);
        assert_eq!(q.submit("b".into()), None);
        assert_eq!(q.pending(), 2);
        assert_eq!(q.mark_ready(), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(q.pending(), 0);
    }

    #[test]
    fn a_ready_page_runs_scripts_at_once() {
        let mut q = ScriptQueue::default();
        q.mark_ready();
        assert_eq!(q.submit("a".into()), Some("a".to_string()));
        assert_eq!(q.pending(), 0);
    }

    #[test]
    fn a_reload_forgets_scripts_meant_for_the_old_page() {
        let mut q = ScriptQueue::default();
        q.submit("old".into());
        q.reset();
        assert!(q.mark_ready().is_empty());
    }

    #[test]
    fn reset_makes_the_page_not_ready_again() {
        let mut q = ScriptQueue::default();
        q.mark_ready();
        q.reset();
        assert!(!q.is_ready());
        assert_eq!(q.submit("x".into()), None);
    }

    #[test]
    fn marking_ready_twice_releases_nothing_the_second_time() {
        let mut q = ScriptQueue::default();
        q.submit("a".into());
        q.mark_ready();
        assert!(q.mark_ready().is_empty());
    }
}
