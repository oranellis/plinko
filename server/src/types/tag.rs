use uuid::Uuid;

#[derive(Debug, PartialEq)]
pub struct Tag {
    id: Uuid,
    name: String,
}

impl Tag {
    pub fn new(name: String) -> Self {
        Self {
            id: Uuid::now_v7(),
            name,
        }
    }
    pub fn id(&self) -> &Uuid {
        &self.id
    }
}
