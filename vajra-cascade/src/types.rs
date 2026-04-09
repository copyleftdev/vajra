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
    pub cascade_ratio: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct CascadeResult {
    pub cascades: Vec<CascadeChain>,
    pub hot_entities: Vec<HotEntity>,
    pub cascade_rate: f64,
    pub self_fix_rate: f64,
    pub total_events: usize,
}
