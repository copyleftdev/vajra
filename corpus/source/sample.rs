use std::collections::BTreeMap;

/// A simple key-value store.
pub struct Store {
    data: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
pub enum Value {
    Text(String),
    Number(f64),
    Flag(bool),
    List(Vec<Value>),
}

impl Store {
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    pub fn set(&mut self, key: String, value: Value) {
        self.data.insert(key, value);
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.data.remove(key)
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn keys(&self) -> Vec<&String> {
        self.data.keys().collect()
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

fn process_values(store: &Store) -> Vec<String> {
    let mut results = Vec::new();
    for key in store.keys() {
        if let Some(value) = store.get(key) {
            match value {
                Value::Text(s) => results.push(format!("{key}: {s}")),
                Value::Number(n) => results.push(format!("{key}: {n}")),
                Value::Flag(b) => results.push(format!("{key}: {b}")),
                Value::List(items) => results.push(format!("{key}: {} items", items.len())),
            }
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_operations() {
        let mut store = Store::new();
        assert!(store.is_empty());

        store.set("name".to_string(), Value::Text("Alice".to_string()));
        store.set("age".to_string(), Value::Number(30.0));
        store.set("active".to_string(), Value::Flag(true));

        assert_eq!(store.len(), 3);
        assert!(!store.is_empty());
    }
}
