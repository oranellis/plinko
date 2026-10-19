// modules {{{
use nanoid::nanoid;
use serde::{Deserialize, Serialize};

use crate::types::error::{PlinkoError, PlinkoResult};
// }}}

#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ShortId([u8; 8]);

// impl ShortId {{{
impl ShortId {
    pub fn new() -> Self {
        Self::from_str_checked(&nanoid!(8)).expect("nanoid output unexpectedly invalid")
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("contained ID unexpectedly invalid")
    }

    pub fn from_str_checked(s: &str) -> PlinkoResult<ShortId> {
        if s.len() == 8 && s.is_ascii() {
            let mut buf = [0u8; 8];
            buf.copy_from_slice(s.as_bytes());
            Ok(Self(buf))
        } else {
            Err(PlinkoError::InvalidShortId(s.to_string()))
        }
    }
}

impl TryFrom<String> for ShortId {
    type Error = PlinkoError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str_checked(&value)
    }
}
// }}}

// impl String {{{
impl From<ShortId> for String {
    fn from(value: ShortId) -> Self {
        String::from_utf8(value.0.to_vec()).expect("contained ID unexpectedly invalid")
    }
}
// }}}

// impl ShortId {{{
impl std::fmt::Display for ShortId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::fmt::Debug for ShortId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ShortId").field(&self.as_str()).finish()
    }
}
// }}}
