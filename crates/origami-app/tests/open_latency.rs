//! Cached-open latency budget check (spec Track A: body paints < 50 ms from
//! an LRU hit). Ignored by default so CI stays deterministic; run explicitly:
//!
//! ```sh
//! cargo test -p origami-app --test open_latency -- --ignored
//! ```

use origami_app_lib::display_lru::{parsed_with_cache, DisplayLru};
use origami_core::message::ParsedMessage;

#[test]
#[ignore = "timing budget check; run with --ignored"]
fn cached_open_under_budget() {
    let mut cache = DisplayLru::new(32 * 1024 * 1024, 300);
    let parsed = ParsedMessage {
        text: Some("warm body".to_string()),
        ..Default::default()
    };
    parsed_with_cache(&mut cache, "k", || Ok(Some(parsed.clone()))).unwrap();

    let iterations = 1_000u32;
    let start = std::time::Instant::now();
    for _ in 0..iterations {
        let hit = parsed_with_cache(&mut cache, "k", || -> origami_core::Result<
            Option<ParsedMessage>,
        > {
            panic!("cache hit must skip the loader");
        })
        .unwrap();
        assert!(hit.is_some());
    }
    let mean = start.elapsed() / iterations;
    assert!(
        mean < std::time::Duration::from_millis(50),
        "mean LRU-hit latency {mean:?} exceeds the 50ms cached-open budget"
    );
}
