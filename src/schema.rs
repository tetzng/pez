use schemars::schema_for;
use serde_json::{Value, json};
use std::{fs, path};

use crate::config::Config;

pub fn generate_config_schema() -> anyhow::Result<Value> {
    let schema = schema_for!(Config);
    let mut value = serde_json::to_value(&schema)?;

    apply_overrides(&mut value)?;
    Ok(value)
}

pub fn write_config_schema(path: &path::Path) -> anyhow::Result<()> {
    let schema = generate_config_schema()?;
    let rendered = serde_json::to_string_pretty(&schema)?;
    fs::write(path, format!("{rendered}\n"))?;
    Ok(())
}

fn apply_overrides(root: &mut Value) -> anyhow::Result<()> {
    let obj = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("schema root must be an object"))?;

    obj.insert(
        "$schema".to_string(),
        json!("http://json-schema.org/draft-07/schema#"),
    );
    obj.insert("title".to_string(), json!("pez config"));
    obj.insert("type".to_string(), json!("object"));
    obj.insert("additionalProperties".to_string(), json!(false));

    let props = obj.entry("properties").or_insert_with(|| json!({}));
    let props_obj = props
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("schema properties must be an object"))?;

    props_obj.insert(
        "plugins".to_string(),
        json!({
            "type": "array",
            "items": plugin_spec_schema()
        }),
    );

    Ok(())
}

fn plugin_spec_schema() -> Value {
    let selector_required = json!({
        "anyOf": [
            { "required": ["version"] },
            { "required": ["branch"] },
            { "required": ["tag"] },
            { "required": ["commit"] }
        ]
    });

    let no_selector = json!({
        "not": selector_required
    });

    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "name": { "type": "string" },
            "repo": {
                "type": "string",
                "pattern": "^(?:[A-Za-z0-9.-]+/)?[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$"
            },
            "url": { "type": "string" },
            "path": {
                "type": "string",
                "pattern": "^(?:/|~(?:/|$))"
            },
            "version": { "type": "string" },
            "branch": { "type": "string" },
            "tag": { "type": "string" },
            "commit": { "type": "string" }
        },
        "allOf": [
            {
                "oneOf": [
                    { "required": ["repo"] },
                    { "required": ["url"] },
                    { "required": ["path"] }
                ]
            },
            {
                "oneOf": [
                    no_selector,
                    { "required": ["version"] },
                    { "required": ["branch"] },
                    { "required": ["tag"] },
                    { "required": ["commit"] }
                ]
            },
            {
                "if": { "required": ["path"] },
                "then": no_selector
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[test]
    fn generated_schema_forbids_selectors_for_path_source() {
        let schema = generate_config_schema().unwrap();
        let plugin_items = schema
            .get("properties")
            .and_then(|value| value.get("plugins"))
            .and_then(|value| value.get("items"))
            .unwrap();
        let all_of = plugin_items.get("allOf").and_then(Value::as_array).unwrap();
        let conditional = all_of
            .iter()
            .find(|entry| entry.get("if").is_some())
            .unwrap();

        assert_eq!(
            conditional.get("if").unwrap(),
            &json!({ "required": ["path"] })
        );
        assert_eq!(
            conditional.get("then").unwrap(),
            &json!({
                "not": {
                    "anyOf": [
                        { "required": ["version"] },
                        { "required": ["branch"] },
                        { "required": ["tag"] },
                        { "required": ["commit"] }
                    ]
                }
            })
        );
    }

    #[test]
    fn write_config_schema_outputs_expected_top_level_keys() {
        let temp = tempfile::tempdir().unwrap();
        let output_path = temp.path().join("schema.json");

        write_config_schema(&output_path).unwrap();

        let content = fs::read_to_string(&output_path).unwrap();
        let schema: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            schema.get("$schema").and_then(Value::as_str),
            Some("http://json-schema.org/draft-07/schema#")
        );
        assert_eq!(
            schema.get("title").and_then(Value::as_str),
            Some("pez config")
        );
        assert!(schema.get("properties").is_some());
    }
}
