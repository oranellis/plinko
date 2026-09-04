use nanoid::nanoid;

#[derive(Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShortId([u8; 8]);

impl ShortId {
    pub fn new() -> Self {
        let id = nanoid!(8);
        let mut buf = [0u8; 8];
        buf.copy_from_slice(id.as_bytes());
        Self(buf)
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("nanoid output is always valid ASCII/UTF-8")
    }
}

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
