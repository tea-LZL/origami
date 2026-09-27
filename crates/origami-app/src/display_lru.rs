//! In-memory LRU of parsed display messages for the open path.

use origami_core::message::ParsedMessage;
use origami_core::Result;
use std::collections::{HashMap, VecDeque};

/// Cache key for a physical source; matches the open-path command signatures.
pub fn display_key(folder_id: &str, server_uid: u32) -> String {
    format!("{folder_id}:{server_uid}")
}

/// Least-recently-used cache of parsed display data, bounded by both a byte
/// budget (estimated from the owned text/html buffers) and an entry count.
/// Recency order: front = least recent, back = most recent.
pub struct DisplayLru {
    max_bytes: usize,
    max_entries: usize,
    entries: HashMap<String, ParsedMessage>,
    bytes_used: usize,
    recency: VecDeque<String>,
}

impl DisplayLru {
    pub fn new(max_bytes: usize, max_entries: usize) -> Self {
        Self {
            max_bytes,
            max_entries,
            entries: HashMap::new(),
            bytes_used: 0,
            recency: VecDeque::new(),
        }
    }

    pub fn get(&mut self, key: &str) -> Option<ParsedMessage> {
        let value = self.entries.get(key)?;
        self.recency.retain(|k| k != key);
        self.recency.push_back(key.to_string());
        Some(value.clone())
    }

    pub fn insert(&mut self, key: String, value: ParsedMessage) {
        let size = estimated_bytes(&value);
        if let Some(old) = self.entries.remove(&key) {
            self.bytes_used = self.bytes_used.saturating_sub(estimated_bytes(&old));
            self.recency.retain(|k| k != &key);
        }
        self.entries.insert(key.clone(), value);
        self.bytes_used += size;
        self.recency.push_back(key);
        self.evict_to_fit();
    }

    pub fn remove(&mut self, key: &str) {
        if let Some(old) = self.entries.remove(key) {
            self.bytes_used = self.bytes_used.saturating_sub(estimated_bytes(&old));
            self.recency.retain(|k| k != key);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.recency.clear();
        self.bytes_used = 0;
    }

    fn evict_to_fit(&mut self) {
        while self.entries.len() > self.max_entries || self.bytes_used > self.max_bytes {
            let Some(oldest) = self.recency.pop_front() else {
                break;
            };
            if let Some(old) = self.entries.remove(&oldest) {
                self.bytes_used = self.bytes_used.saturating_sub(estimated_bytes(&old));
            }
        }
    }
}

/// Serve from cache or load, inserting on hit. Used by the open path so a
/// repeat open never re-reads SQLite or re-parses MIME.
pub fn parsed_with_cache(
    cache: &mut DisplayLru,
    key: &str,
    load: impl FnOnce() -> Result<Option<ParsedMessage>>,
) -> Result<Option<ParsedMessage>> {
    if let Some(value) = cache.get(key) {
        return Ok(Some(value));
    }
    let loaded = load()?;
    if let Some(value) = &loaded {
        cache.insert(key.to_string(), value.clone());
    }
    Ok(loaded)
}

/// Approximate memory footprint: the owned text/html buffers dominate.
fn estimated_bytes(value: &ParsedMessage) -> usize {
    value
        .text
        .as_deref()
        .map_or(0, str::len)
        .saturating_add(value.html.as_deref().map_or(0, str::len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use origami_core::message::ParsedMessage;

    #[test]
    fn display_key_formats_pair() {
        assert_eq!(display_key("f1", 7), "f1:7");
    }

    fn parsed(text: &str) -> ParsedMessage {
        ParsedMessage {
            text: Some(text.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn cache_hit_skips_loader() {
        let mut cache = DisplayLru::new(usize::MAX, 10);
        let calls = std::cell::Cell::new(0);
        let load = || {
            calls.set(calls.get() + 1);
            Ok(Some(parsed("hello")))
        };

        let first = parsed_with_cache(&mut cache, "k", load).unwrap();
        let second = parsed_with_cache(&mut cache, "k", load).unwrap();

        assert!(first.is_some());
        assert!(second.is_some());
        assert_eq!(calls.get(), 1, "second call must be served from cache");
    }

    #[test]
    fn evicts_by_entry_cap() {
        let mut cache = DisplayLru::new(usize::MAX, 3);
        for key in ["a", "b", "c", "d"] {
            cache.insert(key.to_string(), parsed("x"));
        }

        assert!(cache.get("a").is_none(), "oldest entry evicted");
        assert!(cache.get("b").is_some());
        assert!(cache.get("c").is_some());
        assert!(cache.get("d").is_some());
    }

    #[test]
    fn evicts_by_byte_cap() {
        let mut cache = DisplayLru::new(25, usize::MAX);
        cache.insert("a".to_string(), parsed("0123456789")); // 10 bytes
        cache.insert("b".to_string(), parsed("0123456789")); // 20 bytes
        cache.insert("c".to_string(), parsed("0123456789")); // would be 30 > 25

        assert!(
            cache.get("a").is_none(),
            "oldest entry evicted for byte cap"
        );
        assert!(cache.get("b").is_some());
        assert!(cache.get("c").is_some());
    }

    #[test]
    fn get_refreshes_recency() {
        let mut cache = DisplayLru::new(usize::MAX, 2);
        cache.insert("a".to_string(), parsed("x"));
        cache.insert("b".to_string(), parsed("x"));
        assert!(cache.get("a").is_some(), "refreshes a's recency");

        cache.insert("c".to_string(), parsed("x"));

        assert!(cache.get("a").is_some(), "recently used entry survives");
        assert!(cache.get("b").is_none(), "untouched entry evicted");
        assert!(cache.get("c").is_some());
    }
}
