use uuid::Uuid;

#[derive(Debug, PartialEq)]
pub struct Tag {
    id: Uuid,
    name: String,
}

impl Tag {
    pub fn id(&self) -> &Uuid {
        &self.id
    }
}
