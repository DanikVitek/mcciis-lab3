//! Продукційне ядро інтелектуальної системи підтримки прийняття рішень (ІСППР)
//! із прямим логічним виводом (Forward Chaining) та комбінуванням коефіцієнтів
//! впевненості за моделлю MYCIN.

pub mod error;

use core::fmt;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;

use bon::bon;
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

const FLOAT_EQ_EPSILON: f64 = 1e-9;

/// Нормалізує коефіцієнт впевненості у діапазон `[0.0, 1.0]`, захищаючи від `NaN` і нескінченностей.
#[inline]
#[must_use]
fn normalize_cf(certainty_factor: f64) -> f64 {
    if certainty_factor.is_finite() {
        certainty_factor.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Типізоване значення факту в робочій пам'яті ІСППР.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum FactValue {
    /// Логічне значення.
    Bool(bool),
    /// Числове значення з плаваючою комою.
    Number(f64),
    /// Довільне текстове значення.
    Text(String),
}

impl FactValue {
    /// Повертає запозичений зріз рядка без алокацій, якщо факт має варіант [`Self::Text`].
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s.as_str()),
            Self::Bool(_) | Self::Number(_) => None,
        }
    }

    /// Повертає числове подання факту, якщо значення є скінченним числом або парситься як `f64`.
    #[must_use]
    pub fn to_f64(&self) -> Option<f64> {
        match self {
            Self::Number(v) if v.is_finite() => Some(*v),
            Self::Number(_) | Self::Bool(_) => None,
            Self::Text(s) => s.trim().parse::<f64>().ok().filter(|n| n.is_finite()),
        }
    }

    /// Перетворює факт на текстове подання (`Cow<'_, str>`), алокуючи лише для `Number`.
    #[must_use]
    pub fn to_text(&self) -> Cow<'_, str> {
        match self {
            Self::Bool(true) => Cow::Borrowed("true"),
            Self::Bool(false) => Cow::Borrowed("false"),
            Self::Number(n) => Cow::Owned(fmt::from_fn(|f| format_number(f, *n)).to_string()),
            Self::Text(s) => Cow::Borrowed(s.as_str()),
        }
    }

    /// Споживає факт і повертає власний `String`, переміщуючи [`Self::Text`] без копіювання.
    #[must_use]
    pub fn into_text(self) -> String {
        match self {
            Self::Bool(true) => "true".to_owned(),
            Self::Bool(false) => "false".to_owned(),
            Self::Number(n) => fmt::from_fn(|f| format_number(f, n)).to_string(),
            Self::Text(s) => s,
        }
    }

    /// Порівняння значень фактів із допуском для чисел та без урахування регістру ASCII для тексту.
    #[must_use]
    pub fn approx_eq(&self, other: &Self) -> bool {
        match (self.to_f64(), other.to_f64()) {
            (Some(a), Some(b)) => (a - b).abs() < FLOAT_EQ_EPSILON,
            _ => self.to_text().eq_ignore_ascii_case(&other.to_text()),
        }
    }

    /// Розбирає введене користувачем значення у `Bool`, скінченний `Number` або `Text`.
    ///
    /// # Examples
    ///
    /// ```
    /// use dss::FactValue;
    ///
    /// assert_eq!(FactValue::parse_user_input("true"), FactValue::Bool(true));
    /// assert_eq!(FactValue::parse_user_input("42,5"), FactValue::Number(42.5));
    /// assert_eq!(
    ///     FactValue::parse_user_input("elevated"),
    ///     FactValue::Text("elevated".into())
    /// );
    /// ```
    #[must_use]
    pub fn parse_user_input(raw: &str) -> Self {
        let trimmed = raw.trim();
        if trimmed.eq_ignore_ascii_case("true") {
            return Self::Bool(true);
        }
        if trimmed.eq_ignore_ascii_case("false") {
            return Self::Bool(false);
        }
        let parsed = if trimmed.contains(',') {
            trimmed.replace(',', ".").parse::<f64>()
        } else {
            trimmed.parse::<f64>()
        };
        if let Ok(num) = parsed
            && num.is_finite()
        {
            return Self::Number(num);
        }
        Self::Text(trimmed.to_string())
    }
}

fn format_number(f: &mut fmt::Formatter<'_>, n: f64) -> fmt::Result {
    if !n.is_finite() {
        fmt::Display::fmt(&n, f)
    } else if n.fract().abs() < f64::EPSILON {
        fmt::Display::fmt(&n.trunc(), f)
    } else {
        fmt::Display::fmt(&(n.trunc() + (n.fract() * 10000.0).trunc() / 10000.0), f)
    }
}

impl From<&str> for FactValue {
    fn from(raw: &str) -> Self {
        Self::parse_user_input(raw)
    }
}

impl fmt::Display for FactValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(b) => fmt::Display::fmt(b, f),
            Self::Text(s) => fmt::Display::fmt(s, f),
            Self::Number(n) => format_number(f, *n),
        }
    }
}

/// Походження факту в робочій пам'яті ІСППР.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactOrigin {
    /// Зібрано автономним Web-агентом.
    WebAgent,
    /// Виведено машиною логічного виводу ІСППР.
    Inferred,
    /// Додано користувачем вручну (what-if симуляція).
    Manual,
}

impl FactOrigin {
    /// Повертає локалізовану назву походження факту українською.
    #[must_use]
    pub const fn label_uk(self) -> &'static str {
        match self {
            Self::WebAgent => "Web-агент",
            Self::Inferred => "Виведено ІСППР",
            Self::Manual => "Користувач",
        }
    }
}

/// Факт робочої пам'яті ІСППР із джерелом та коефіцієнтом впевненості.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Fact {
    pub key: String,
    pub value: FactValue,
    pub source: String,
    #[serde(rename = "cf")]
    pub certainty_factor: f64,
    pub origin: FactOrigin,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl Fact {
    /// Створює новий факт із нормалізацією коефіцієнта впевненості у `[0.0, 1.0]`.
    #[must_use]
    pub fn new(
        key: impl Into<String>,
        value: FactValue,
        source: impl Into<String>,
        certainty_factor: f64,
        origin: FactOrigin,
    ) -> Self {
        Self {
            key: key.into(),
            value,
            source: source.into(),
            certainty_factor: normalize_cf(certainty_factor),
            origin,
            updated_at: OffsetDateTime::now_utc(),
        }
    }
}

/// Оператор порівняння умови продукційного правила.
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
    /// Повертає символьне подання оператора.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Ne => "≠",
            Self::Gt => ">",
            Self::Gte => "≥",
            Self::Lt => "<",
            Self::Lte => "≤",
            Self::Contains => "∋",
        }
    }

    /// Перевіряє, чи задовольняє фактичне значення `actual` умові щодо `expected`.
    #[must_use]
    pub fn matches(self, actual: &FactValue, expected: &FactValue) -> bool {
        match self {
            Self::Eq => actual.approx_eq(expected),
            Self::Ne => !actual.approx_eq(expected),
            Self::Gt => matches!(
                (actual.to_f64(), expected.to_f64()),
                (Some(a), Some(b)) if a > b
            ),
            Self::Gte => matches!(
                (actual.to_f64(), expected.to_f64()),
                (Some(a), Some(b)) if a >= b
            ),
            Self::Lt => matches!(
                (actual.to_f64(), expected.to_f64()),
                (Some(a), Some(b)) if a < b
            ),
            Self::Lte => matches!(
                (actual.to_f64(), expected.to_f64()),
                (Some(a), Some(b)) if a <= b
            ),
            Self::Contains => actual
                .to_text()
                .to_lowercase()
                .contains(&expected.to_text().to_lowercase()),
        }
    }
}

/// Окрема умова (антецедент) продукційного правила.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub fact: String,
    pub op: Operator,
    pub value: FactValue,
}

/// Специфікація виведеного факту (консеквента) правила.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedFactSpec {
    pub key: String,
    pub value: FactValue,
}

/// Рівень критичності рішення ІСППР.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Medium,
    High,
    Critical,
}

impl Severity {
    /// Повертає локалізовану назву рівня критичності українською.
    #[must_use]
    pub const fn label_uk(self) -> &'static str {
        match self {
            Self::Critical => "Критичний",
            Self::High => "Високий",
            Self::Medium => "Середній",
            Self::Info => "Інформаційний",
        }
    }

    /// Повертає CSS-клас для відображення рівня критичності в інтерфейсі.
    #[must_use]
    pub const fn css_class(self) -> &'static str {
        match self {
            Self::Critical => "sev-critical",
            Self::High => "sev-high",
            Self::Medium => "sev-medium",
            Self::Info => "sev-info",
        }
    }
}

/// Продукційне правило бази знань ІСППР.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(try_from = "RawRule")]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub priority: i32,
    pub severity: Severity,
    pub certainty_factor: f64,
    pub conditions: Vec<Condition>,
    pub derived_fact: Option<DerivedFactSpec>,
    pub recommendation: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    priority: i32,
    severity: Severity,
    certainty_factor: f64,
    conditions: Vec<Condition>,
    derived_fact: Option<DerivedFactSpec>,
    recommendation: String,
}

impl TryFrom<RawRule> for Rule {
    type Error = DssError;

    fn try_from(raw: RawRule) -> Result<Self, Self::Error> {
        Self::builder()
            .id(raw.id)
            .name(raw.name)
            .description(raw.description)
            .priority(raw.priority)
            .severity(raw.severity)
            .certainty_factor(raw.certainty_factor)
            .conditions(raw.conditions)
            .maybe_derived_fact(raw.derived_fact)
            .recommendation(raw.recommendation)
            .build()
    }
}

#[bon]
impl Rule {
    /// Створює та валідує продукційне правило через типобезпечний builder (`bon`).
    ///
    /// # Errors
    ///
    /// Повертає [`DssError::InvalidRule`], якщо `id` порожній, список умов порожній
    /// або `cf` не є скінченним числом у межах `[0.0, 1.0]`.
    #[builder(derive(Debug), on(String, into))]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        name: String,
        #[builder(default)] description: String,
        #[builder(default)] priority: i32,
        severity: Severity,
        certainty_factor: f64,
        conditions: Vec<Condition>,
        derived_fact: Option<DerivedFactSpec>,
        recommendation: String,
    ) -> DssResult<Self> {
        if id.trim().is_empty() {
            return Err(DssError::InvalidRule(
                "ідентифікатор правила не може бути порожнім".to_string(),
            ));
        }
        if conditions.is_empty() {
            return Err(DssError::InvalidRule(format!(
                "правило `{id}` повинно містити хоча б одну умову"
            )));
        }
        if !certainty_factor.is_finite() || !(0.0..=1.0).contains(&certainty_factor) {
            return Err(DssError::InvalidRule(format!(
                "коефіцієнт впевненості CF правила `{id}` має бути в межах [0.0; 1.0]"
            )));
        }
        Ok(Self {
            id,
            name,
            description,
            priority,
            severity,
            certainty_factor,
            conditions,
            derived_fact,
            recommendation,
        })
    }
}

/// База знань ІСППР із гарантованою унікальністю ідентифікаторів правил.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(try_from = "RawKnowledgeBase")]
pub struct KnowledgeBase {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// Створює базу знань, перевіряючи відсутність дубльованих ідентифікаторів правил.
    ///
    /// # Errors
    ///
    /// Повертає [`DssError::InvalidRule`], якщо серед правил є дубльовані `id`.
    pub fn new(rules: Vec<Rule>) -> DssResult<Self> {
        {
            let mut ids = HashSet::with_capacity(rules.len());
            for rule in &rules {
                if !ids.insert(rule.id.as_str()) {
                    return Err(DssError::InvalidRule(format!(
                        "дубльований ідентифікатор правила БЗ: `{}`",
                        rule.id
                    )));
                }
            }
        }
        Ok(Self { rules })
    }

    /// Завантажує та валідує базу знань із TOML-файлу на диску.
    ///
    /// # Errors
    ///
    /// Повертає [`DssError::Io`] при помилці читання файлу або [`DssError::TomlDe`]
    /// при помилці синтаксису чи валідації правил.
    pub fn load_from_file(path: impl AsRef<Path>) -> DssResult<Self> {
        let raw = std::fs::read_to_string(path)?;
        let kb: KnowledgeBase = toml::from_str(&raw)?;
        Ok(kb)
    }
}

/// Трасування окремої спрацьованої умови правила (підсистема пояснення ІСППР).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConditionTrace {
    pub fact_key: String,
    pub operator: Operator,
    pub expected: FactValue,
    pub actual: FactValue,
    pub fact_source: String,
    pub fact_cf: f64,
}

/// Рішення ІСППР, сформоване внаслідок спрацювання продукційного правила.
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

/// Результат повного циклу логічного виводу ІСППР.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct InferenceResult {
    pub decisions: Vec<Decision>,
    pub inferred_facts: BTreeMap<String, Fact>,
    pub fired_rule_ids: Vec<String>,
    pub iterations: usize,
}

/// Машина прямого логічного виводу (Forward Chaining + MYCIN CF).
#[derive(Debug, Clone)]
pub struct InferenceEngine {
    kb: KnowledgeBase,
    ordered_indices: Vec<usize>,
}

impl InferenceEngine {
    /// Ініціалізує машину виводу та попередньо впорядковує індекси правил за пріоритетом.
    #[must_use]
    pub fn new(kb: KnowledgeBase) -> Self {
        let mut ordered_indices: Vec<usize> = (0..kb.rules.len()).collect();
        ordered_indices.sort_by(|&a, &b| {
            let ra = &kb.rules[a];
            let rb = &kb.rules[b];
            rb.priority
                .cmp(&ra.priority)
                .then_with(|| ra.id.cmp(&rb.id))
        });
        Self {
            kb,
            ordered_indices,
        }
    }

    /// Повертає зріз правил активної бази знань.
    #[must_use]
    pub fn rules(&self) -> &[Rule] {
        &self.kb.rules
    }

    /// Комбінування коефіцієнтів впевненості за моделлю MYCIN (Шортліфф):
    /// `CF_comb = CF1 + CF2 * (1 - CF1)` для позитивних свідчень у `[0, 1]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use dss::InferenceEngine;
    ///
    /// let combined = InferenceEngine::combine_cf(0.8, 0.8);
    /// assert!((combined - 0.96).abs() < 1e-12);
    /// ```
    #[must_use]
    pub fn combine_cf(cf1: f64, cf2: f64) -> f64 {
        let a = normalize_cf(cf1);
        let b = normalize_cf(cf2);
        normalize_cf(a + b * (1.0 - a))
    }

    /// Прямий логічний вивід (Forward Chaining) до досягнення нерухомої точки.
    #[must_use]
    pub fn evaluate(&self, base_facts: &BTreeMap<String, Fact>) -> InferenceResult {
        let mut working_memory = base_facts.clone();
        let mut inferred_facts: BTreeMap<String, Fact> = BTreeMap::new();
        let mut decisions: Vec<Decision> = Vec::new();
        let mut fired_rules: HashSet<&str> = HashSet::with_capacity(self.kb.rules.len());

        let max_iterations = self.kb.rules.len().saturating_add(2);
        let mut iterations = 0;

        for _ in 0..max_iterations {
            iterations += 1;
            let mut changed = false;

            for &idx in &self.ordered_indices {
                let rule = &self.kb.rules[idx];
                if fired_rules.contains(rule.id.as_str()) {
                    continue;
                }

                if let Some((min_cond_cf, traces)) = check_conditions(rule, &working_memory) {
                    let decision_cf = normalize_cf(min_cond_cf * rule.certainty_factor);
                    fired_rules.insert(rule.id.as_str());
                    changed = true;

                    if let Some(derived) = &rule.derived_fact {
                        match working_memory.get_mut(&derived.key) {
                            Some(existing)
                                if existing.origin == FactOrigin::Inferred
                                    && existing.value.approx_eq(&derived.value) =>
                            {
                                existing.certainty_factor =
                                    Self::combine_cf(existing.certainty_factor, decision_cf);
                                existing.source.push_str(", ");
                                existing.source.push_str(&rule.id);
                                existing.updated_at = OffsetDateTime::now_utc();
                                inferred_facts.insert(derived.key.clone(), existing.clone());
                            }
                            Some(existing)
                                if existing.origin == FactOrigin::Inferred
                                    && decision_cf > existing.certainty_factor =>
                            {
                                let updated = Fact::new(
                                    derived.key.clone(),
                                    derived.value.clone(),
                                    format!("Правило {}", rule.id),
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
                                    format!("Правило {}", rule.id),
                                    decision_cf,
                                    FactOrigin::Inferred,
                                );
                                working_memory.insert(derived.key.clone(), new_fact.clone());
                                inferred_facts.insert(derived.key.clone(), new_fact);
                            }
                            _ => {}
                        }
                    }

                    decisions.push(Decision {
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        priority: rule.priority,
                        severity: rule.severity,
                        cf: decision_cf,
                        recommendation: rule.recommendation.clone(),
                        derived_fact: rule.derived_fact.clone(),
                        explanation: traces,
                    });
                }
            }

            if !changed {
                break;
            }
        }

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
        min_cf = min_cf.min(normalize_cf(fact.certainty_factor));
        traces.push(ConditionTrace {
            fact_key: cond.fact.clone(),
            operator: cond.op,
            expected: cond.value.clone(),
            actual: fact.value.clone(),
            fact_source: fact.source.clone(),
            fact_cf: fact.certainty_factor,
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

    #[test]
    fn non_finite_cf_is_normalized_to_zero() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let fact = Fact::new("k", FactValue::Bool(true), "src", bad, FactOrigin::Manual);
            assert!(fact.certainty_factor.is_finite());
            assert!((0.0..=1.0).contains(&fact.certainty_factor));

            let combined = InferenceEngine::combine_cf(bad, 0.5);
            assert!(combined.is_finite());
            assert!((combined - 0.5).abs() < 1e-12);
        }
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
            let value = f64::from(num) + (f64::from(frac) / 10_000.0) * if num < 0 { -1.0 } else { 1.0 };
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
            prop_assert!((0.0..=1.0).contains(&fact.certainty_factor));
        }

        #[test]
        fn rule_builder_enforces_cf_bounds(cf in -5.0_f64..5.0_f64) {
            let res = Rule::builder()
                .id("R1")
                .name("Rule")
                .severity(Severity::Info)
                .certainty_factor(cf)
                .conditions(vec![Condition {
                    fact: "x".into(),
                    op: Operator::Eq,
                    value: FactValue::Bool(true),
                }])
                .recommendation("Rec")
                .build();
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
                let rule = Rule::builder()
                    .id(format!("R_{i:02}"))
                    .name(format!("Step {i}"))
                    .priority(priorities[i])
                    .severity(severities[i])
                    .certainty_factor(rule_cfs[i])
                    .conditions(vec![Condition {
                        fact: format!("step_{i}"),
                        op: Operator::Eq,
                        value: FactValue::Bool(true),
                    }])
                    .derived_fact(DerivedFactSpec {
                        key: format!("step_{}", i + 1),
                        value: FactValue::Bool(true),
                    })
                    .recommendation(format!("Action {i}"))
                    .build()
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
                prop_assert!((derived.certainty_factor - running_cf).abs() < 1e-12);
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
                prop_assert!(ord != core::cmp::Ordering::Greater);
            }
        }

        #[test]
        fn duplicate_consequent_rules_combine_cf_via_mycin(
            fact_cf in 0.1_f64..=1.0_f64,
            cf1 in 0.1_f64..=1.0_f64,
            cf2 in 0.1_f64..=1.0_f64,
        ) {
            let make_rule = |id: &str, cf: f64| {
                Rule::builder()
                    .id(id)
                    .name(id)
                    .priority(10)
                    .severity(Severity::High)
                    .certainty_factor(cf)
                    .conditions(vec![Condition {
                        fact: "signal".into(),
                        op: Operator::Eq,
                        value: FactValue::Bool(true),
                    }])
                    .derived_fact(DerivedFactSpec {
                        key: "risk".into(),
                        value: FactValue::Text("elevated".into()),
                    })
                    .recommendation("Act")
                    .build()
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
            let actual_cf = res.inferred_facts["risk"].certainty_factor;
            prop_assert!((actual_cf - expected_cf).abs() < 1e-12);
        }
    }
}
