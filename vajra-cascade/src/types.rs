use serde::Serialize;
#[derive(Debug, Clone)]
pub struct CascadeConfig {
    pub entity_field: String,
    pub time_field: String,
    pub event_field: String,
    pub trigger_values: Vec<String>,
    pub response_values: Vec<String>,
}
impl CascadeConfig {
    pub fn is_response(&self, value: &str) -> bool {
        self.response_values
            .iter()
            .any(|rv| value.contains(rv.as_str()))
    }
}
impl Default for CascadeConfig {
    fn default() -> Self {
        Self {
            entity_field: "file".to_owned(),
            time_field: "date".to_owned(),
            event_field: "intent".to_owned(),
            trigger_values: Vec::new(),
            response_values: vec!["fix".to_owned(), "revert".to_owned()],
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct TriggerResponse {
    pub value: String,
    pub author: String,
    pub time: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct CascadeChain {
    pub entity: String,
    pub trigger: TriggerResponse,
    pub response: TriggerResponse,
    pub same_author: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct HotEntity {
    pub entity: String,
    pub total: usize,
    pub cascades: usize,
    /// Raw `cascades / total`. Kept for inspection; not the ranking key.
    pub cascade_ratio: f64,
    /// Wilson score lower bound on `cascade_ratio` at 95%, and the key
    /// `hot_entities` is sorted by.
    ///
    /// The raw ratio cannot be ranked: an entity touched twice with one
    /// response scores 50% and outranks one touched nineteen times with seven,
    /// though it evidences nothing. The bound is what the support will support.
    pub cascade_ratio_lower_bound: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct CascadeResult {
    pub cascades: Vec<CascadeChain>,
    /// Sorted by `cascade_ratio_lower_bound`, descending.
    pub hot_entities: Vec<HotEntity>,
    pub cascade_rate: f64,
    /// Fraction of cascades whose response has the same author as the trigger.
    ///
    /// `None` when the entity field selects the author: cascades are then
    /// within one author by construction and the rate is 1.0 regardless of the
    /// data. See `self_fix_rate_note`.
    pub self_fix_rate: Option<f64>,
    /// Why `self_fix_rate` is absent, when it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_fix_rate_note: Option<String>,
    pub total_events: usize,
}
