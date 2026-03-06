use std::collections::HashSet;
use serde::{Deserialize, Serialize};
use crate::data::ids::UserId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub name: String,
    /// Tags representing this user's skills, roles, or clearances.
    pub tags: HashSet<String>,
}

impl User {
    pub fn new(name: impl Into<String>) -> Self {
        Self { id: UserId::new(), name: name.into(), tags: HashSet::new() }
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.insert(tag.into());
        self
    }

    pub fn add_tag(&mut self, tag: impl Into<String>) {
        self.tags.insert(tag.into());
    }

    pub fn remove_tag(&mut self, tag: &str) {
        self.tags.remove(tag);
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.contains(tag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_user_has_no_tags() {
        let u = User::new("Alice");
        assert!(u.tags.is_empty());
    }

    #[test]
    fn add_tag_and_has_tag() {
        let mut u = User::new("Alice");
        u.add_tag("rust");
        assert!(u.has_tag("rust"));
        assert!(!u.has_tag("python"));
    }

    #[test]
    fn remove_tag() {
        let mut u = User::new("Alice");
        u.add_tag("rust");
        u.remove_tag("rust");
        assert!(!u.has_tag("rust"));
    }

    #[test]
    fn with_tag_builder() {
        let u = User::new("Alice").with_tag("rust").with_tag("skia");
        assert!(u.has_tag("rust"));
        assert!(u.has_tag("skia"));
        assert!(!u.has_tag("python"));
    }

    #[test]
    fn duplicate_tags_are_deduplicated() {
        let mut u = User::new("Alice");
        u.add_tag("rust");
        u.add_tag("rust");
        assert_eq!(u.tags.len(), 1);
    }
}
