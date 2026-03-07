//! The [`User`] type — a team member with skill/role tags used for affinity matching.

use std::collections::HashSet;
use serde::{Deserialize, Serialize};
use crate::data::ids::UserId;

/// A team member who can be assigned to tasks.
///
/// `tags` represent skills, roles, or clearances (e.g. `"rust"`, `"designer"`).
/// Tasks declare `required_tags`; a user is eligible only if they hold every
/// required tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub name: String,
    /// Skills, roles, or clearances this user possesses.
    pub tags: HashSet<String>,
}

impl User {
    /// Creates a user with no tags.
    pub fn new(name: impl Into<String>) -> Self {
        Self { id: UserId::new(), name: name.into(), tags: HashSet::new() }
    }

    /// Builder: adds a tag and returns `self`.  Useful for chained construction.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.insert(tag.into());
        self
    }

    /// Inserts a tag into this user's tag set.
    pub fn add_tag(&mut self, tag: impl Into<String>) {
        self.tags.insert(tag.into());
    }

    /// Removes a tag from this user's tag set.
    pub fn remove_tag(&mut self, tag: &str) {
        self.tags.remove(tag);
    }

    /// Returns `true` if this user possesses the given tag.
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
