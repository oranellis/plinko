use uuid::Uuid;

#[derive(Debug, PartialEq)]
pub struct Tag {
    id: Uuid,
    name: String,
}
