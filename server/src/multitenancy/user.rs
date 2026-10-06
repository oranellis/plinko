// modules{{{
use uuid::Uuid;
// }}}

pub struct TenantLoginUser {
    id: Uuid,
    name: String,
    email: String,
}

// impl TenantLoginUser{{{
impl From<NewTenantLoginUser> for TenantLoginUser {
    fn from(new: NewTenantLoginUser) -> Self {
        Self {
            id: new.id,
            name: new.name,
            email: new.email,
        }
    }
}

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

pub struct NewTenantLoginUser {
    pub id: Uuid,
    pub name: String,
    pub email: String,
}

pub struct TenantLoginUserPatch {
    pub id: Uuid,
    pub name: Option<String>,
    pub email: Option<String>,
}
