use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Clone)]
struct SessionModelEntry {
    model: String,
    updated_at: Instant,
}

pub struct SessionModelRegistry {
    entries: Mutex<HashMap<String, SessionModelEntry>>,
    max_entries: usize,
    ttl: Duration,
}

impl SessionModelRegistry {
    pub fn new(max_entries: usize, ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            max_entries: max_entries.max(1),
            ttl,
        }
    }

    pub fn record(&self, session_id: &str, model: &str) {
        let now = Instant::now();
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.retain(|_, entry| now.duration_since(entry.updated_at) <= self.ttl);

        if !entries.contains_key(session_id) && entries.len() >= self.max_entries {
            if let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.updated_at)
                .map(|(session_id, _)| session_id.clone())
            {
                entries.remove(&oldest);
            }
        }

        entries.insert(
            session_id.to_string(),
            SessionModelEntry {
                model: model.to_string(),
                updated_at: now,
            },
        );
    }

    pub fn resolve(&self, session_id: &str) -> Option<String> {
        let now = Instant::now();
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.retain(|_, entry| now.duration_since(entry.updated_at) <= self.ttl);
        entries.get(session_id).map(|entry| entry.model.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::SessionModelRegistry;
    use std::{thread, time::Duration};

    #[test]
    fn records_updates_and_isolates_session_models() {
        let registry = SessionModelRegistry::new(1024, Duration::from_secs(7200));

        registry.record("session-a", "claude-haiku-4-5");
        registry.record("session-b", "claude-opus-4-8");
        assert_eq!(
            registry.resolve("session-a").as_deref(),
            Some("claude-haiku-4-5")
        );
        assert_eq!(
            registry.resolve("session-b").as_deref(),
            Some("claude-opus-4-8")
        );

        registry.record("session-a", "claude-sonnet-4-6");
        assert_eq!(
            registry.resolve("session-a").as_deref(),
            Some("claude-sonnet-4-6")
        );
        assert_eq!(
            registry.resolve("session-b").as_deref(),
            Some("claude-opus-4-8")
        );
    }

    #[test]
    fn expires_stale_entries() {
        let registry = SessionModelRegistry::new(1024, Duration::from_millis(1));
        registry.record("session-a", "claude-haiku-4-5");
        thread::sleep(Duration::from_millis(10));

        assert_eq!(registry.resolve("session-a"), None);
    }

    #[test]
    fn evicts_oldest_entry_at_capacity() {
        let registry = SessionModelRegistry::new(2, Duration::from_secs(7200));
        registry.record("oldest", "claude-haiku-4-5");
        thread::sleep(Duration::from_millis(2));
        registry.record("newer", "claude-sonnet-4-6");
        thread::sleep(Duration::from_millis(2));
        registry.record("newest", "claude-opus-4-8");

        assert_eq!(registry.resolve("oldest"), None);
        assert_eq!(
            registry.resolve("newer").as_deref(),
            Some("claude-sonnet-4-6")
        );
        assert_eq!(
            registry.resolve("newest").as_deref(),
            Some("claude-opus-4-8")
        );
    }
}
