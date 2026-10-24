//! Helpers for reading JSON out of free-form LLM replies.

use serde_json::Value;

/// Extracts the first JSON array/object from text that may be wrapped in ``` fences or prose.
pub fn extract_json(text: &str) -> Option<Value> {
    let t = text.trim();
    if let Ok(v) = serde_json::from_str::<Value>(t) {
        return Some(v);
    }
    let start = t.find(|c| c == '[' || c == '{')?;
    let open = t.as_bytes()[start] as char;
    let close = if open == '[' { ']' } else { '}' };
    let end = t.rfind(close)?;
    if end <= start {
        return None;
    }
    serde_json::from_str::<Value>(&t[start..=end]).ok()
}

/// Reads a list of items from either `["a","b"]`, `[{"keyword":"a","intent":"x"}]`
/// or `{"keywords":[...]}`. Returns (text, optional label).
pub fn string_items(v: &Value, text_keys: &[&str], label_key: &str) -> Vec<(String, Option<String>)> {
    let arr = match v {
        Value::Array(a) => a.clone(),
        Value::Object(m) => m.values().find_map(|x| x.as_array().cloned()).unwrap_or_default(),
        _ => Vec::new(),
    };
    arr.iter()
        .filter_map(|item| match item {
            Value::String(s) => Some((s.trim().to_string(), None)),
            Value::Object(m) => {
                let text = text_keys.iter().find_map(|k| m.get(*k).and_then(|x| x.as_str()))?;
                let label = m.get(label_key).and_then(|x| x.as_str()).map(|s| s.to_string());
                Some((text.trim().to_string(), label))
            }
            _ => None,
        })
        .filter(|(t, _)| !t.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_from_fences_and_prose() {
        let v = extract_json("Sure!\n```json\n[\"a\", \"b\"]\n```\nDone").unwrap();
        assert_eq!(v.as_array().unwrap().len(), 2);
        assert!(extract_json("no json here").is_none());
        assert!(extract_json("{\"a\":1}").is_some());
    }

    #[test]
    fn reads_items() {
        let v = extract_json("{\"keywords\":[{\"keyword\":\"x\",\"intent\":\"commercial\"},\"y\",{\"nope\":1}]}").unwrap();
        let items = string_items(&v, &["keyword"], "intent");
        assert_eq!(items, vec![("x".to_string(), Some("commercial".to_string())), ("y".to_string(), None)]);
    }
}
