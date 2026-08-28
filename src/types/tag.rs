use uuid::Uuid;

pub struct Tag {
    id: Uuid,
    name: String,
}

pub type TagUuid = Uuid;
