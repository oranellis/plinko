use std::collections::HashMap;

use plinko::{multitenancy::tenant::*, planner::plan::Plan, types::short_id::ShortId};

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
