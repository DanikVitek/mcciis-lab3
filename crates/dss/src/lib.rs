pub mod error;

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use hashbrown::HashSet;
use serde::{
    Deserialize,
    Serialize,
};
use time::OffsetDateTime;

use crate::error::{
    DssError,
    DssResult,
};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum FactValue {
    Bool(bool),
    Number(f64),
    Text(String),
}

impl FactValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(v) => Some(*v),
            Self::Text(s) => s.trim().parse::<f64>().ok(),
            Self::Bool(_) => None,
        }
    }

    pub fn as_text(&self) -> String {
        match self {
            Self::Bool(b) => b.to_string(),
            Self::Number(n) => {
                if n.fract().abs() < f64::EPSILON {
                    format!("{n:.0}")
                } else {
                    format!("{n:.4}")
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                        .to_string()
                }
            }
            Self::Text(s) => s.clone(),
        }
    }

    pub fn parse_user_input(raw: &str) -> Self {
        let trimmed = raw.trim();
        if trimmed.eq_ignore_ascii_case("true") {
            return Self::Bool(true);
        }
        if trimmed.eq_ignore_ascii_case("false") {
            return Self::Bool(false);
        }
        if let Ok(num) = trimmed.replace(',', ".").parse::<f64>()
            && num.is_finite()
        {
            return Self::Number(num);
        }
        Self::Text(trimmed.to_string())
    }
}

impl fmt::Display for FactValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_text())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactOrigin {
    WebAgent,
    Inferred,
    Manual,
}

impl FactOrigin {
    pub fn label_uk(self) -> &'static str {
        match self {
            Self::WebAgent => "Web-агент",
            Self::Inferred => "Виведено ІСППР",
            Self::Manual => "Користувач",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Fact {
    pub key: String,
    pub value: FactValue,
    pub source: String,
    pub cf: f64,
    pub origin: FactOrigin,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl Fact {
    pub fn new(
        key: impl Into<String>,
        value: FactValue,
        source: impl Into<String>,
        cf: f64,
        origin: FactOrigin,
    ) -> Self {
        Self {
            key: key.into(),
            value,
            source: source.into(),
            cf: cf.clamp(0.0, 1.0),
            origin,
            updated_at: OffsetDateTime::now_utc(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Operator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Contains,
}

impl Operator {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::Contains => "містить",
        }
    }

    pub fn matches(self, actual: &FactValue, expected: &FactValue) -> bool {
        match self {
            Self::Eq => match (actual.as_f64(), expected.as_f64()) {
                (Some(a), Some(b)) => (a - b).abs() < 1e-9,
                _ => actual.as_text().eq_ignore_ascii_case(&expected.as_text()),
            },
            Self::Ne => !Self::Eq.matches(actual, expected),
            Self::Gt => matches!(
                (actual.as_f64(), expected.as_f64()),
                (Some(a), Some(b)) if a > b
            ),
            Self::Gte => matches!(
                (actual.as_f64(), expected.as_f64()),
                (Some(a), Some(b)) if a >= b
            ),
            Self::Lt => matches!(
                (actual.as_f64(), expected.as_f64()),
                (Some(a), Some(b)) if a < b
            ),
            Self::Lte => matches!(
                (actual.as_f64(), expected.as_f64()),
                (Some(a), Some(b)) if a <= b
            ),
            Self::Contains => actual
                .as_text()
                .to_lowercase()
                .contains(&expected.as_text().to_lowercase()),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Condition {
    pub fact: String,
    pub op: Operator,
    pub value: FactValue,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DerivedFactSpec {
    pub key: String,
    pub value: FactValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn label_uk(self) -> &'static str {
        match self {
            Self::Critical => "Критичний",
            Self::High => "Високий",
            Self::Medium => "Середній",
            Self::Info => "Інформаційний",
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            Self::Critical => "sev-critical",
            Self::High => "sev-high",
            Self::Medium => "sev-medium",
            Self::Info => "sev-info",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(try_from = "RawRule")]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub priority: i32,
    pub severity: Severity,
    pub cf: f64,
    pub conditions: Vec<Condition>,
    pub derived_fact: Option<DerivedFactSpec>,
    pub recommendation: String,
}

#[derive(Debug, Deserialize)]
struct RawRule {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    priority: i32,
    severity: Severity,
    cf: f64,
    conditions: Vec<Condition>,
    derived_fact: Option<DerivedFactSpec>,
    recommendation: String,
}

impl TryFrom<RawRule> for Rule {
    type Error = DssError;

    fn try_from(raw: RawRule) -> Result<Self, Self::Error> {
        Self::new(
            raw.id,
            raw.name,
            raw.description,
            raw.priority,
            raw.severity,
            raw.cf,
            raw.conditions,
            raw.derived_fact,
            raw.recommendation,
        )
    }
}

impl Rule {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        name: String,
        description: String,
        priority: i32,
        severity: Severity,
        cf: f64,
        conditions: Vec<Condition>,
        derived_fact: Option<DerivedFactSpec>,
        recommendation: String,
    ) -> DssResult<Self> {
        if id.trim().is_empty() {
            return Err(DssError::InvalidRule(
                "Ідентифікатор правила не може бути порожнім".to_string(),
            ));
        }
        if conditions.is_empty() {
            return Err(DssError::InvalidRule(format!(
                "Правило `{id}` повинно містити хоча б одну умову"
            )));
        }
        if !cf.is_finite() || !(0.0..=1.0).contains(&cf) {
            return Err(DssError::InvalidRule(format!(
                "Коефіцієнт впевненості CF правила `{id}` має бути в межах [0.0; 1.0]"
            )));
        }
        Ok(Self {
            id,
            name,
            description,
            priority,
            severity,
            cf,
            conditions,
            derived_fact,
            recommendation,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(try_from = "RawKnowledgeBase")]
pub struct KnowledgeBase {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Deserialize)]
struct RawKnowledgeBase {
    rules: Vec<Rule>,
}

impl TryFrom<RawKnowledgeBase> for KnowledgeBase {
    type Error = DssError;

    fn try_from(raw: RawKnowledgeBase) -> Result<Self, Self::Error> {
        Self::new(raw.rules)
    }
}

impl KnowledgeBase {
    pub fn new(rules: Vec<Rule>) -> DssResult<Self> {
        {
            let mut ids = HashSet::new();
            for rule in &rules {
                if !ids.insert(rule.id.as_str()) {
                    return Err(DssError::InvalidRule(format!(
                        "Дубльований ідентифікатор правила БЗ: `{}`",
                        rule.id
                    )));
                }
            }
        }
        Ok(Self { rules })
    }

    pub fn load_from_file(path: impl AsRef<Path>) -> DssResult<Self> {
        let raw = std::fs::read_to_string(path)?;
        let kb: KnowledgeBase = toml::from_str(&raw)?;
        Ok(kb)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConditionTrace {
    pub fact_key: String,
    pub operator: Operator,
    pub expected: FactValue,
    pub actual: FactValue,
    pub fact_source: String,
    pub fact_cf: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Decision {
    pub rule_id: String,
    pub rule_name: String,
    pub priority: i32,
    pub severity: Severity,
    pub cf: f64,
    pub recommendation: String,
    pub derived_fact: Option<DerivedFactSpec>,
    pub explanation: Vec<ConditionTrace>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct InferenceResult {
    pub decisions: Vec<Decision>,
    pub inferred_facts: BTreeMap<String, Fact>,
    pub fired_rule_ids: Vec<String>,
    pub iterations: usize,
}

#[derive(Debug, Clone)]
pub struct InferenceEngine {
    kb: KnowledgeBase,
}

impl InferenceEngine {
    pub fn new(kb: KnowledgeBase) -> Self {
        Self { kb }
    }

    pub fn rules(&self) -> &[Rule] {
        &self.kb.rules
    }

    /// Комбінування коефіцієнтів впевненості за моделлю MYCIN (Шортліфф):
    /// `CF_comb = CF1 + CF2 * (1 - CF1)` для позитивних свідчень у `[0, 1]`.
    pub fn combine_cf(cf1: f64, cf2: f64) -> f64 {
        let a = cf1.clamp(0.0, 1.0);
        let b = cf2.clamp(0.0, 1.0);
        (a + b * (1.0 - a)).clamp(0.0, 1.0)
    }

    /// Прямий логічний вивід (Forward Chaining) до досягнення нерухомої точки.
    pub fn evaluate(&self, base_facts: &BTreeMap<String, Fact>) -> InferenceResult {
        let mut working_memory = base_facts.clone();
        let mut inferred_facts: BTreeMap<String, Fact> = BTreeMap::new();
        let mut decisions_by_rule: BTreeMap<String, Decision> = BTreeMap::new();
        let mut fired_rules: HashSet<String> = HashSet::new();

        let mut ordered_rules: Vec<&Rule> = self.kb.rules.iter().collect();
        ordered_rules.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));

        let max_iterations = self.kb.rules.len().saturating_add(2);
        let mut iterations = 0;

        for _ in 0..max_iterations {
            iterations += 1;
            let mut changed = false;

            for rule in &ordered_rules {
                if fired_rules.contains(&rule.id) {
                    continue;
                }

                if let Some((min_cond_cf, traces)) = check_conditions(rule, &working_memory) {
                    let decision_cf = (min_cond_cf * rule.cf).clamp(0.0, 1.0);
                    fired_rules.insert(rule.id.clone());
                    changed = true;

                    if let Some(derived) = &rule.derived_fact {
                        let source_label = format!("Правило {}", rule.id);
                        match working_memory.get_mut(&derived.key) {
                            Some(existing)
                                if existing.origin == FactOrigin::Inferred
                                    && existing.value == derived.value =>
                            {
                                existing.cf = Self::combine_cf(existing.cf, decision_cf);
                                existing.source = format!("{}, {}", existing.source, rule.id);
                                existing.updated_at = OffsetDateTime::now_utc();
                                inferred_facts.insert(derived.key.clone(), existing.clone());
                            }
                            Some(existing)
                                if existing.origin == FactOrigin::Inferred
                                    && decision_cf > existing.cf =>
                            {
                                let updated = Fact::new(
                                    derived.key.clone(),
                                    derived.value.clone(),
                                    source_label,
                                    decision_cf,
                                    FactOrigin::Inferred,
                                );
                                *existing = updated.clone();
                                inferred_facts.insert(derived.key.clone(), updated);
                            }
                            None => {
                                let new_fact = Fact::new(
                                    derived.key.clone(),
                                    derived.value.clone(),
                                    source_label,
                                    decision_cf,
                                    FactOrigin::Inferred,
                                );
                                working_memory.insert(derived.key.clone(), new_fact.clone());
                                inferred_facts.insert(derived.key.clone(), new_fact);
                            }
                            _ => {}
                        }
                    }

                    decisions_by_rule.insert(
                        rule.id.clone(),
                        Decision {
                            rule_id: rule.id.clone(),
                            rule_name: rule.name.clone(),
                            priority: rule.priority,
                            severity: rule.severity,
                            cf: decision_cf,
                            recommendation: rule.recommendation.clone(),
                            derived_fact: rule.derived_fact.clone(),
                            explanation: traces,
                        },
                    );
                }
            }

            if !changed {
                break;
            }
        }

        let mut decisions: Vec<Decision> = decisions_by_rule.into_values().collect();
        decisions.sort_by(|a, b| {
            b.severity
                .cmp(&a.severity)
                .then_with(|| b.priority.cmp(&a.priority))
                .then_with(|| b.cf.total_cmp(&a.cf))
                .then_with(|| a.rule_id.cmp(&b.rule_id))
        });

        let fired_rule_ids = decisions.iter().map(|d| d.rule_id.clone()).collect();

        InferenceResult {
            decisions,
            inferred_facts,
            fired_rule_ids,
            iterations,
        }
    }
}

fn check_conditions(
    rule: &Rule,
    working_memory: &BTreeMap<String, Fact>,
) -> Option<(f64, Vec<ConditionTrace>)> {
    let mut min_cf = 1.0_f64;
    let mut traces = Vec::with_capacity(rule.conditions.len());

    for cond in &rule.conditions {
        let fact = working_memory.get(&cond.fact)?;
        if !cond.op.matches(&fact.value, &cond.value) {
            return None;
        }
        min_cf = min_cf.min(fact.cf);
        traces.push(ConditionTrace {
            fact_key: cond.fact.clone(),
            operator: cond.op,
            expected: cond.value.clone(),
            actual: fact.value.clone(),
            fact_source: fact.source.clone(),
            fact_cf: fact.cf,
        });
    }

    Some((min_cf, traces))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn arb_fact_value() -> impl Strategy<Value = FactValue> {
        prop_oneof![
            any::<bool>().prop_map(FactValue::Bool),
            (-1e6_f64..1e6_f64).prop_map(FactValue::Number),
            "[A-Za-z0-9 _-]{0,24}".prop_map(FactValue::Text),
        ]
    }

    fn arb_severity() -> impl Strategy<Value = Severity> {
        prop_oneof![
            Just(Severity::Info),
            Just(Severity::Medium),
            Just(Severity::High),
            Just(Severity::Critical),
        ]
    }

    proptest! {
        #[test]
        fn combine_cf_satisfies_mycin_algebraic_laws(
            a in 0.0_f64..=1.0_f64,
            b in 0.0_f64..=1.0_f64,
            c in 0.0_f64..=1.0_f64,
        ) {
            let ab = InferenceEngine::combine_cf(a, b);
            let ba = InferenceEngine::combine_cf(b, a);
            prop_assert!((0.0..=1.0).contains(&ab));
            prop_assert!((ab - ba).abs() < 1e-12);
            prop_assert!(ab + 1e-12 >= a.max(b));

            let lhs = InferenceEngine::combine_cf(ab, c);
            let rhs = InferenceEngine::combine_cf(a, InferenceEngine::combine_cf(b, c));
            prop_assert!((lhs - rhs).abs() < 1e-12);

            prop_assert!((InferenceEngine::combine_cf(a, 0.0) - a).abs() < 1e-12);
            prop_assert!((InferenceEngine::combine_cf(a, 1.0) - 1.0).abs() < 1e-12);
        }

        #[test]
        fn combine_cf_clamps_out_of_range_inputs(
            a in -100.0_f64..100.0_f64,
            b in -100.0_f64..100.0_f64,
        ) {
            let combined = InferenceEngine::combine_cf(a, b);
            prop_assert!((0.0..=1.0).contains(&combined));
        }

        #[test]
        fn operator_eq_ne_and_numeric_order_laws(
            x in -1e6_f64..1e6_f64,
            y in -1e6_f64..1e6_f64,
        ) {
            let vx = FactValue::Number(x);
            let vy = FactValue::Number(y);

            prop_assert!(Operator::Eq.matches(&vx, &vx));
            prop_assert_eq!(Operator::Eq.matches(&vx, &vy), Operator::Eq.matches(&vy, &vx));
            prop_assert_eq!(Operator::Ne.matches(&vx, &vy), !Operator::Eq.matches(&vx, &vy));

            prop_assert_eq!(Operator::Gt.matches(&vx, &vy), Operator::Lt.matches(&vy, &vx));
            prop_assert_eq!(Operator::Gte.matches(&vx, &vy), Operator::Lte.matches(&vy, &vx));
            prop_assert!(!(Operator::Gt.matches(&vx, &vy) && Operator::Lt.matches(&vx, &vy)));
            prop_assert!(Operator::Gte.matches(&vx, &vy) || Operator::Lt.matches(&vx, &vy));
            prop_assert!(Operator::Lte.matches(&vx, &vy) || Operator::Gt.matches(&vx, &vy));
        }

        #[test]
        fn operator_ne_is_negation_of_eq_for_arbitrary_fact_values(
            a in arb_fact_value(),
            b in arb_fact_value(),
        ) {
            prop_assert_eq!(Operator::Ne.matches(&a, &b), !Operator::Eq.matches(&a, &b));
        }

        #[test]
        fn operator_contains_is_reflexive_and_case_insensitive(
            prefix in "[a-zA-Z0-9 ]{0,12}",
            needle in "[a-zA-Z0-9]{1,12}",
            suffix in "[a-zA-Z0-9 ]{0,12}",
        ) {
            let haystack = FactValue::Text(format!("{prefix}{needle}{suffix}"));
            let upper = FactValue::Text(needle.to_ascii_uppercase());
            let lower = FactValue::Text(needle.to_ascii_lowercase());

            prop_assert!(Operator::Contains.matches(&haystack, &haystack));
            prop_assert!(Operator::Contains.matches(&haystack, &upper));
            prop_assert!(Operator::Contains.matches(&haystack, &lower));
        }

        #[test]
        fn parse_user_input_roundtrips_numbers_and_booleans(
            num: i32,
            frac in 0_u16..10_000_u16,
            use_comma: bool,
            flag: bool,
            pad_left in "[ \t]{0,3}",
            pad_right in "[ \t]{0,3}",
        ) {
            let value = num as f64 + (frac as f64 / 10_000.0) * if num < 0 { -1.0 } else { 1.0 };
            let formatted = if use_comma {
                format!("{pad_left}{value:.4}{pad_right}").replace('.', ",")
            } else {
                format!("{pad_left}{value:.4}{pad_right}")
            };
            match FactValue::parse_user_input(&formatted) {
                FactValue::Number(parsed) => prop_assert!((parsed - value).abs() < 1e-3),
                other => prop_assert!(false, "expected Number, got {other:?}"),
            }

            let bool_raw = format!("{pad_left}{}{pad_right}", if flag { "TrUe" } else { "FaLsE" });
            prop_assert_eq!(FactValue::parse_user_input(&bool_raw), FactValue::Bool(flag));
        }

        #[test]
        fn fact_new_always_clamps_cf(cf in -10.0_f64..10.0_f64) {
            let fact = Fact::new("k", FactValue::Bool(true), "src", cf, FactOrigin::Manual);
            prop_assert!((0.0..=1.0).contains(&fact.cf));
        }

        #[test]
        fn rule_new_enforces_cf_bounds(cf in -5.0_f64..5.0_f64) {
            let cond = vec![Condition {
                fact: "x".into(),
                op: Operator::Eq,
                value: FactValue::Bool(true),
            }];
            let res = Rule::new(
                "R1".into(),
                "Rule".into(),
                String::new(),
                0,
                Severity::Info,
                cf,
                cond,
                None,
                "Rec".into(),
            );
            prop_assert_eq!(res.is_ok(), (0.0..=1.0).contains(&cf));
        }

        #[test]
        fn forward_chaining_propagates_arbitrary_length_chain_and_preserves_ordering(
            chain_len in 1_usize..=12,
            base_cf in 0.1_f64..=1.0_f64,
            rule_cfs in prop::collection::vec(0.1_f64..=1.0_f64, 12),
            priorities in prop::collection::vec(any::<i32>(), 12),
            severities in prop::collection::vec(arb_severity(), 12),
        ) {
            let mut rules = Vec::with_capacity(chain_len);
            for i in 0..chain_len {
                let rule = Rule::new(
                    format!("R_{i:02}"),
                    format!("Step {i}"),
                    String::new(),
                    priorities[i],
                    severities[i],
                    rule_cfs[i],
                    vec![Condition {
                        fact: format!("step_{i}"),
                        op: Operator::Eq,
                        value: FactValue::Bool(true),
                    }],
                    Some(DerivedFactSpec {
                        key: format!("step_{}", i + 1),
                        value: FactValue::Bool(true),
                    }),
                    format!("Action {i}"),
                )
                .unwrap();
                rules.push(rule);
            }

            // Перевертаємо порядок правил, щоб перевірити незалежність прямого виводу від порядку в БЗ
            rules.reverse();
            let kb = KnowledgeBase::new(rules).unwrap();
            let engine = InferenceEngine::new(kb);

            let mut base_facts = BTreeMap::new();
            base_facts.insert(
                "step_0".to_string(),
                Fact::new("step_0", FactValue::Bool(true), "test", base_cf, FactOrigin::WebAgent),
            );

            let res = engine.evaluate(&base_facts);
            prop_assert!(res.iterations <= chain_len + 2);
            prop_assert_eq!(res.decisions.len(), chain_len);
            prop_assert_eq!(res.inferred_facts.len(), chain_len);

            let expected_ids: Vec<String> = res.decisions.iter().map(|d| d.rule_id.clone()).collect();
            prop_assert_eq!(res.fired_rule_ids, expected_ids);

            // Кожне виведене правило має очікуваний добуток CF уздовж ланцюжка
            let mut running_cf = base_cf;
            for i in 0..chain_len {
                running_cf = (running_cf * rule_cfs[i]).clamp(0.0, 1.0);
                let derived = &res.inferred_facts[&format!("step_{}", i + 1)];
                prop_assert!((derived.cf - running_cf).abs() < 1e-12);
                prop_assert_eq!(derived.origin, FactOrigin::Inferred);
            }

            // Рішення строго відсортовані за (severity desc, priority desc, cf desc, rule_id asc)
            for pair in res.decisions.windows(2) {
                let (a, b) = (&pair[0], &pair[1]);
                let ord = b
                    .severity
                    .cmp(&a.severity)
                    .then_with(|| b.priority.cmp(&a.priority))
                    .then_with(|| b.cf.total_cmp(&a.cf))
                    .then_with(|| a.rule_id.cmp(&b.rule_id));
                prop_assert!(ord != std::cmp::Ordering::Greater);
            }
        }

        #[test]
        fn duplicate_consequent_rules_combine_cf_via_mycin(
            fact_cf in 0.1_f64..=1.0_f64,
            cf1 in 0.1_f64..=1.0_f64,
            cf2 in 0.1_f64..=1.0_f64,
        ) {
            let make_rule = |id: &str, cf: f64| {
                Rule::new(
                    id.into(),
                    id.into(),
                    String::new(),
                    10,
                    Severity::High,
                    cf,
                    vec![Condition {
                        fact: "signal".into(),
                        op: Operator::Eq,
                        value: FactValue::Bool(true),
                    }],
                    Some(DerivedFactSpec {
                        key: "risk".into(),
                        value: FactValue::Text("elevated".into()),
                    }),
                    "Act".into(),
                )
                .unwrap()
            };

            let kb = KnowledgeBase::new(vec![make_rule("R1", cf1), make_rule("R2", cf2)]).unwrap();
            let engine = InferenceEngine::new(kb);

            let mut facts = BTreeMap::new();
            facts.insert(
                "signal".into(),
                Fact::new("signal", FactValue::Bool(true), "test", fact_cf, FactOrigin::WebAgent),
            );

            let res = engine.evaluate(&facts);
            let expected_cf = InferenceEngine::combine_cf(fact_cf * cf1, fact_cf * cf2);
            let actual_cf = res.inferred_facts["risk"].cf;
            prop_assert!((actual_cf - expected_cf).abs() < 1e-12);
        }
    }
}
