use std::collections::BTreeMap;

use dss::{
    Fact,
    FactOrigin,
    FactValue,
    InferenceEngine,
    KnowledgeBase,
    Severity,
};

#[test]
fn deserialization_validates_rules_via_factory() {
    let invalid_cf = r#"
[[rules]]
id = "R1"
name = "Invalid CF"
severity = "high"
cf = 1.5
conditions = [{ fact = "x", op = "gt", value = 1.0 }]
recommendation = "Test"
"#;
    assert!(toml::from_str::<KnowledgeBase>(invalid_cf).is_err());

    let empty_conditions = r#"
[[rules]]
id = "R2"
name = "No conditions"
severity = "info"
cf = 0.8
conditions = []
recommendation = "Test"
"#;
    assert!(toml::from_str::<KnowledgeBase>(empty_conditions).is_err());

    let duplicate_ids = r#"
[[rules]]
id = "DUP"
name = "One"
severity = "info"
cf = 0.8
conditions = [{ fact = "x", op = "eq", value = 1.0 }]
recommendation = "A"

[[rules]]
id = "DUP"
name = "Two"
severity = "info"
cf = 0.7
conditions = [{ fact = "y", op = "eq", value = 2.0 }]
recommendation = "B"
"#;
    assert!(toml::from_str::<KnowledgeBase>(duplicate_ids).is_err());
}

#[test]
fn forward_chaining_and_mycin_cf_combination() {
    let kb = KnowledgeBase::load_from_file("../../knowledge/rules.toml")
        .expect("knowledge/rules.toml should parse");
    let engine = InferenceEngine::new(kb);

    let mut facts = BTreeMap::new();
    facts.insert(
        "cve_mentions_count".into(),
        Fact::new(
            "cve_mentions_count",
            FactValue::Number(2.0),
            "test",
            1.0,
            FactOrigin::WebAgent,
        ),
    );
    facts.insert(
        "cyber_critical_articles".into(),
        Fact::new(
            "cyber_critical_articles",
            FactValue::Number(1.0),
            "test",
            0.9,
            FactOrigin::WebAgent,
        ),
    );
    facts.insert(
        "nbu_rate_usd".into(),
        Fact::new(
            "nbu_rate_usd",
            FactValue::Number(42.5),
            "nbu",
            1.0,
            FactOrigin::WebAgent,
        ),
    );
    facts.insert(
        "nbu_rate_eur".into(),
        Fact::new(
            "nbu_rate_eur",
            FactValue::Number(46.0),
            "nbu",
            1.0,
            FactOrigin::WebAgent,
        ),
    );

    let result = engine.evaluate(&facts);

    // Обидва правила (USD і EUR) підтверджують fx_pressure_level = "elevated" → CF комбінується за MYCIN:
    // 0.8 + 0.8 * (1 - 0.8) = 0.96
    let fx_fact = result
        .inferred_facts
        .get("fx_pressure_level")
        .expect("fx_pressure_level must be inferred");
    assert_eq!(fx_fact.value, FactValue::Text("elevated".into()));
    assert!((fx_fact.cf - 0.96).abs() < 1e-6);

    // Ланцюговий вивід (Level 2): R_META_CRISIS_MODE активується на основі виведених фактів
    let op_mode = result
        .inferred_facts
        .get("operational_mode")
        .expect("operational_mode must be inferred");
    assert_eq!(op_mode.value, FactValue::Text("crisis_response".into()));
    assert!(result.iterations >= 2);

    // Найперше рішення має критичний рівень та непусте пояснення
    let top = &result.decisions[0];
    assert_eq!(top.severity, Severity::Critical);
    assert!(!top.explanation.is_empty());
}
