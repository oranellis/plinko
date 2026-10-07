use chrono::NaiveDate;
use plinko::{
    multitenancy::tenant::Tenant,
    scheduler::ledger::{Claim, Ledger, MAX_FILL_DAYS, Run},
    types::short_id::ShortId,
};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

#[test]
fn tenant_ledgers_start_with_each_users_calendar() {
    let start = d(2024, 1, 1);
    let mut tenant = Tenant::new(ShortId::new(), "Tenant".to_string(), start);
    let user = *tenant.users().keys().next().unwrap();

    // without calendars nobody has hours
    assert_eq!(Ledger::from_tenant(&tenant, start).free(&user, start), 0);

    let until = start + chrono::Days::new(u64::from(MAX_FILL_DAYS));
    tenant.refresh_calendars(start, until);
    let mut ledger = Ledger::from_tenant(&tenant, start);
    assert_eq!(ledger.free(&user, start), 8);
    assert_eq!(ledger.free(&user, d(2024, 1, 6)), 0, "Saturday");
    let run = Run(vec![Claim {
        user,
        day: start,
        hours: 8,
    }]);
    assert!(ledger.claim(&run));
    assert!(!ledger.claim(&run));
}
