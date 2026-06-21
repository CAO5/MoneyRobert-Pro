use chrono::Duration;
use moneyrobert_rs::auth::Claims;

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
