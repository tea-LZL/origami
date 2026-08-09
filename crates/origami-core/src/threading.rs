//! Threading: References-based conversation grouping.
//!
//! Union-find over Message-ID/References/In-Reply-To chains — the
//! root-grouping core of jwz's algorithm (subject merging arrives with
//! the conversation UI in M5). The thread id of a message is the
//! Message-ID of the oldest reachable ancestor, giving stable,
//! deterministic grouping as long as ancestors are recorded first.

use std::collections::HashMap;

#[derive(Default)]
pub struct Threader {
    /// Union-find: child message-id → parent message-id.
    parent: HashMap<String, String>,
}

impl Threader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a message and its ancestry (`references` oldest→newest,
    /// optionally followed by In-Reply-To as the last element).
    pub fn add(&mut self, message_id: &str, references: &[String]) {
        self.ensure(message_id);
        let mut previous: Option<&str> = None;
        for reference in references {
            self.ensure(reference);
            if let Some(older) = previous {
                // The newer reference's parent is the older one.
                self.union(reference, older);
            }
            previous = Some(reference);
        }
        if let Some(parent) = previous {
            self.union(message_id, parent);
        }
    }

    /// Root message-id of the thread containing `message_id`.
    pub fn root_for(&mut self, message_id: &str) -> String {
        self.ensure(message_id);
        self.find(message_id)
    }

    fn ensure(&mut self, id: &str) {
        self.parent
            .entry(id.to_string())
            .or_insert_with(|| id.to_string());
    }

    fn find(&mut self, id: &str) -> String {
        let mut root = id.to_string();
        let mut guard = 0usize;
        loop {
            let parent = self.parent[&root].clone();
            if parent == root {
                break;
            }
            root = parent;
            guard += 1;
            if guard > self.parent.len() {
                // Cycle (broken References): cut at the current node.
                break;
            }
        }
        // Path compression.
        let mut node = id.to_string();
        while self.parent[&node] != node && self.parent[&node] != root {
            let next = self.parent[&node].clone();
            self.parent.insert(node, root.clone());
            node = next;
        }
        root
    }

    fn union(&mut self, child: &str, parent: &str) {
        let child_root = self.find(child);
        let parent_root = self.find(parent);
        if child_root != parent_root {
            // Parent (older reference) becomes the root of the child.
            self.parent.insert(child_root, parent_root);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn standalone_message_is_its_own_root() {
        let mut t = Threader::new();
        t.add("a@x", &[]);
        assert_eq!(t.root_for("a@x"), "a@x");
    }

    #[test]
    fn reply_chains_share_the_ancestor_root() {
        let mut t = Threader::new();
        t.add("a@x", &[]);
        t.add("b@x", &refs(&["a@x"]));
        t.add("c@x", &refs(&["a@x", "b@x"]));
        assert_eq!(t.root_for("b@x"), "a@x");
        assert_eq!(t.root_for("c@x"), "a@x");
    }

    #[test]
    fn out_of_order_arrival_still_groups() {
        let mut t = Threader::new();
        // Reply arrives before the original was recorded.
        t.add("c@x", &refs(&["a@x", "b@x"]));
        t.add("a@x", &[]);
        assert_eq!(t.root_for("c@x"), "a@x");
        assert_eq!(t.root_for("b@x"), "a@x");
    }

    #[test]
    fn cyclic_references_do_not_hang() {
        let mut t = Threader::new();
        t.add("a@x", &refs(&["b@x"]));
        t.add("b@x", &refs(&["a@x"]));
        // Any deterministic root is acceptable; termination is the test.
        let _ = t.root_for("a@x");
    }

    #[test]
    fn sibling_replies_group_together() {
        let mut t = Threader::new();
        t.add("root@x", &[]);
        t.add("r1@x", &refs(&["root@x"]));
        t.add("r2@x", &refs(&["root@x"]));
        assert_eq!(t.root_for("r1@x"), t.root_for("r2@x"));
    }
}
