use chrono::NaiveDate;
use plinko::{planner::plan::*, types::short_id::ShortId};

#[test]
fn create_plan() {
    let id = ShortId::from_str("ABCDEFGH").expect("should always be valid");
    let start_date = NaiveDate::from_ymd_opt(2000, 12, 31).unwrap();
    let plan = Plan::new(id, "My Plan".to_owned(), start_date);

    assert_eq!(plan.id().to_owned(), id);
    assert_eq!(plan.name(), "My Plan");
}
