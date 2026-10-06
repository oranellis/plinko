use std::collections::HashMap;

use plinko::{
    multitenancy::{tenant::*, user::TenantLoginUser},
    planner::plan::Plan,
    types::short_id::ShortId,
};

#[test]
fn create_tenant_with_root_user() {
    let tenant_id = ShortId::new();
    let tenant_with_root = Tenant::new(tenant_id.clone(), "Example Tenant".to_string());
    assert_eq!(*tenant_with_root.id(), tenant_id);
    assert_eq!(tenant_with_root.name(), "Example Tenant");
    assert_eq!(*tenant_with_root.plans(), HashMap::<ShortId, Plan>::new());
    let users = tenant_with_root.users();
    assert!(users.len() == 1);
    let (_, user) = users.iter().next().unwrap();
    assert_eq!(user.name(), "Root User");
    assert_eq!(user.email(), "root@localhost");
}

#[test]
fn create_tenant_with_login() {
    let user = TenantLoginUser::new("Alice".to_string(), "alice@example.com".to_string());
    let user_id = *user.id();
    let tenant = Tenant::new_with_login(ShortId::new(), "Example Tenant".to_string(), user);
    assert_eq!(tenant.users().len(), 1);
    assert_eq!(tenant.users()[&user_id].email(), "alice@example.com");
}
