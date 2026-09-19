// modules{{{
use uuid::Uuid;
// }}}

pub struct TenantLoginUser {
    id: Uuid,
    name: String,
    email: String,
}

// impl TenantLoginUser{{{
impl TenantLoginUser {
    pub fn new(name: String, email: String) -> Self {
        Self {
            id: Uuid::now_v7(),
            name,
            email,
        }
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn set_email(&mut self, email: String) {
        self.email = email;
    }
}
// }}}
