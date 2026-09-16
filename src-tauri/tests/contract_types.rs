use serde_json::json;

#[test]
fn usage_snapshot_serializes_to_contract_shape() {
    let snap = ollamabar_lib::types::UsageSnapshot {
        session_pct: Some(9.2),
        weekly_pct: None,
        session_reset_est: Some("2026-09-15T10:00:00Z".into()),
        weekly_reset_est: None,
        session_models: vec![ollamabar_lib::types::ModelStat { name: "gpt-oss:120b".into(), request_count: 242 }],
        weekly_models: vec![],
        server_time: Some("2026-09-15T06:29:56Z".into()),
        fetched_at: "2026-09-15T06:30:00Z".into(),
        last_success_at: Some("2026-09-15T06:30:00Z".into()),
        failure_level: ollamabar_lib::types::FailureLevel::None,
        error_message: None,
    };
    let v = serde_json::to_value(&snap).unwrap();
    assert_eq!(v["session_pct"], json!(9.2));
    assert_eq!(v["weekly_pct"], json!(null));
    assert_eq!(v["failure_level"], json!("none"));
    assert_eq!(v["session_models"][0]["request_count"], json!(242));
}