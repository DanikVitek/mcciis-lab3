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
