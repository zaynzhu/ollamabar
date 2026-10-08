use serde_json::json;
use ollamabar_lib::types::PlanType;

#[test]
fn usage_snapshot_serializes_to_contract_shape() {
    let snap = ollamabar_lib::types::UsageSnapshot {
        plan_type: PlanType::Legacy,
        session_pct: Some(9.2),
        weekly_pct: None,
        session_reset_est: Some("2026-09-15T10:00:00Z".into()),
        weekly_reset_est: None,
        session_reset_from_server: true,
        weekly_reset_from_server: false,
        session_models: vec![ollamabar_lib::types::ModelStat { name: "gpt-oss:120b".into(), request_count: 242 }],
        weekly_models: vec![],
        models_available: false,
        balance_usd: None,
        allowance_usd: None,
        period_from: None,
        period_until: None,
        requests_24h: None,
        requests_7d: Some(612),
        requests_30d: None,
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
    assert_eq!(v["session_models"][0]["request_count"], json!(242)); // 模型明细字段保留（官方恢复前置点）
    // 2026-10 契约扩展
    assert_eq!(v["plan_type"], json!("legacy"));
    assert_eq!(v["models_available"], json!(false));
    assert_eq!(v["session_reset_from_server"], json!(true));
    assert_eq!(v["weekly_reset_from_server"], json!(false));
    assert_eq!(v["requests_24h"], json!(null));
    assert_eq!(v["requests_7d"], json!(612));
    assert_eq!(v["requests_30d"], json!(null));
    assert_eq!(v["balance_usd"], json!(null));
    assert_eq!(v["allowance_usd"], json!(null));
    assert_eq!(v["period_from"], json!(null));
    assert_eq!(v["period_until"], json!(null));
}

#[test]
fn plan_type_usage_based序列化() {
    assert_eq!(serde_json::to_value(PlanType::UsageBased).unwrap(), json!("usage_based"));
    assert_eq!(serde_json::to_value(PlanType::Legacy).unwrap(), json!("legacy"));
}