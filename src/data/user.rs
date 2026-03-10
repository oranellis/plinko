//! The [`User`] type — a team member with skill/role tags used for affinity matching.

use crate::data::ids::UserId;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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
    /// Raw image bytes (any Skia-decodable format: JPEG, PNG, WebP, …).
    ///
    /// Serialised as a base64 string in JSON snapshots.  `None` when no avatar
    /// is set.  Old snapshots that lack the field deserialise to `None`.
    #[serde(with = "avatar_serde", default)]
    pub avatar: Option<Vec<u8>>,
}

impl User {
    /// Creates a user with no tags and no avatar.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: UserId::new(),
            name: name.into(),
            tags: HashSet::new(),
            avatar: None,
        }
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

// ── base64 codec (no external crate) ─────────────────────────────────────────

mod base64 {
    const ENC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(src: &[u8]) -> String {
        let mut out = String::with_capacity(src.len().div_ceil(3) * 4);
        for chunk in src.chunks(3) {
            let n = chunk.len();
            let b0 = chunk[0] as u32;
            let b1 = if n > 1 { chunk[1] as u32 } else { 0 };
            let b2 = if n > 2 { chunk[2] as u32 } else { 0 };
            let v = (b0 << 16) | (b1 << 8) | b2;
            out.push(ENC[(v >> 18 & 63) as usize] as char);
            out.push(ENC[(v >> 12 & 63) as usize] as char);
            out.push(if n > 1 {
                ENC[(v >> 6 & 63) as usize] as char
            } else {
                '='
            });
            out.push(if n > 2 {
                ENC[(v & 63) as usize] as char
            } else {
                '='
            });
        }
        out
    }

    pub fn decode(s: &str) -> Result<Vec<u8>, &'static str> {
        let b = s.as_bytes();
        if !b.len().is_multiple_of(4) {
            return Err("invalid base64 length");
        }
        let mut out = Vec::with_capacity(b.len() / 4 * 3);
        for quad in b.chunks(4) {
            let c0 = val(quad[0])?;
            let c1 = val(quad[1])?;
            out.push((c0 << 2) | (c1 >> 4));
            if quad[2] != b'=' {
                let c2 = val(quad[2])?;
                out.push(((c1 & 0xf) << 4) | (c2 >> 2));
                if quad[3] != b'=' {
                    let c3 = val(quad[3])?;
                    out.push(((c2 & 0x3) << 6) | c3);
                }
            }
        }
        Ok(out)
    }

    fn val(c: u8) -> Result<u8, &'static str> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err("invalid base64 character"),
        }
    }
}

mod avatar_serde {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            None => s.serialize_none(),
            Some(bytes) => s.serialize_some(&super::base64::encode(bytes)),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        let opt: Option<String> = Option::deserialize(d)?;
        match opt {
            None => Ok(None),
            Some(s) => super::base64::decode(&s)
                .map(Some)
                .map_err(serde::de::Error::custom),
        }
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

    #[test]
    fn avatar_base64_round_trip() {
        let data: Vec<u8> = (0u8..=255).collect();
        let encoded = super::base64::encode(&data);
        let decoded = super::base64::decode(&encoded).unwrap();
        assert_eq!(data, decoded);
    }

    #[test]
    fn avatar_serde_round_trip() {
        let mut u = User::new("Bob");
        u.avatar = Some(b"hello world".to_vec());
        let json = serde_json::to_string(&u).unwrap();
        let u2: User = serde_json::from_str(&json).unwrap();
        assert_eq!(u2.avatar, u.avatar);
    }

    #[test]
    fn avatar_default_none_on_old_json() {
        // JSON without the avatar field should deserialise with avatar = None
        let json = r#"{"id":"00000000-0000-0000-0000-000000000000","name":"Eve","tags":[]}"#;
        let u: User = serde_json::from_str(json).unwrap();
        assert!(u.avatar.is_none());
    }
}
