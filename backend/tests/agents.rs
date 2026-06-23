use chrono::Duration;
use moneyrobert_rs::auth::Claims;

#[path = "agents/autonomous_tests.rs"]
mod autonomous_tests;
#[path = "agents/debate_tests.rs"]
mod debate_tests;
#[path = "agents/decision_tests.rs"]
mod decision_tests;
#[path = "agents/promotion_tests.rs"]
mod promotion_tests;
#[path = "agents/risk_tests.rs"]
mod risk_tests;
#[path = "agents/simulation_tests.rs"]
mod simulation_tests;

#[test]
fn access_claims_are_typed_as_access_tokens() {
    let claims = Claims::new(
        1,
        "alice".to_string(),
        "normal".to_string(),
        Duration::minutes(5),
    );
    assert!(claims.is_access_token());
    assert!(!claims.is_refresh_token());
}

#[test]
fn refresh_claims_are_typed_as_refresh_tokens() {
    let claims = Claims::new_refresh(
        1,
        "alice".to_string(),
        "normal".to_string(),
        Duration::days(1),
    );
    assert!(claims.is_refresh_token());
    assert!(!claims.is_access_token());
}
