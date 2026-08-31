use uuid::Uuid;

pub struct TenantUser {
    id: TenantUserUuid,
    email: String,
}

pub type TenantUserUuid = Uuid;
