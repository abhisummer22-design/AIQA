//! Phase 8B — checks a JSON value against the part of JSON Schema that an OpenAPI response really uses, and says what is wrong.
//!
//! An API test that only compares one field with one value cannot say "the response has the shape the specification promises".
//! This can: types, required properties, nested properties and items, enums, string, number and array bounds, a pattern, and
//! `allOf`/`anyOf`/`oneOf`. Everything else in a schema (a format, a discriminator, `readOnly`, an unknown keyword) is left
//! alone rather than guessed at, and a `$ref` that was not resolved beforehand (`openapi_spec::resolve`) is not checked.
//!
//! What it does not do, on purpose: `oneOf` means at least one alternative matches (a sample can fit two), `additionalProperties`
//! is only enforced when it is `false`, and `not` is ignored. The answer is a list of violations, each with the path of the value
//! it is about (`$.owner.name`, `$.items[2].id`), so a failed test can say what was wrong, not only that something was.

use regex::Regex;
use serde_json::Value;

/// How many violations are reported before it stops: a response that is wrong everywhere is not a report of hundreds of lines.
pub const MAX_VIOLATIONS: usize = 20;

/// What is wrong with `value` as a `schema`; empty when it fits.
pub fn violations(schema: &Value, value: &Value) -> Vec<String> {
    let mut found = Vec::new();
    check(schema, value, "$", &mut found);
    found.truncate(MAX_VIOLATIONS);
    found
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        // Every integer is a float with nothing after the point (and so is `5.0`, which JSON schemas count as an integer).
        Value::Number(n) if n.as_f64().is_some_and(|f| f.fract() == 0.0) => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn is_type(value: &Value, wanted: &str) -> bool {
    match wanted {
        // An integer is a number too.
        "number" => matches!(value, Value::Number(_)),
        other => type_name(value) == other,
    }
}

/// The types a schema allows, if it names any: `"string"`, or a list of them (OpenAPI 3.1), and `null` too when it is `nullable`.
fn allowed_types(schema: &serde_json::Map<String, Value>) -> Option<Vec<String>> {
    let mut types: Vec<String> = match schema.get("type")? {
        Value::String(t) => vec![t.clone()],
        Value::Array(list) => list
            .iter()
            .filter_map(|t| t.as_str().map(str::to_string))
            .collect(),
        _ => return None,
    };
    if schema.get("nullable").and_then(Value::as_bool) == Some(true) {
        types.push("null".to_string());
    }
    Some(types)
}

fn check(schema: &Value, value: &Value, path: &str, found: &mut Vec<String>) {
    if found.len() >= MAX_VIOLATIONS {
        return;
    }
    let Some(schema) = schema.as_object() else {
        return;
    };

    if let Some(types) = allowed_types(schema) {
        if !types.is_empty() && !types.iter().any(|t| is_type(value, t)) {
            found.push(format!(
                "{path}: expected {}, got {}",
                types.join(" or "),
                type_name(value)
            ));
            // Whatever else the schema says about the value is about a value of the wrong kind.
            return;
        }
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array) {
        if !options.contains(value) {
            found.push(format!(
                "{path}: {} is not one of the allowed values",
                short(value)
            ));
        }
    }
    if let Some(expected) = schema.get("const") {
        if expected != value {
            found.push(format!(
                "{path}: expected {}, got {}",
                short(expected),
                short(value)
            ));
        }
    }

    for part in schema
        .get("allOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        check(part, value, path, found);
    }
    for group in ["anyOf", "oneOf"] {
        if let Some(options) = schema
            .get(group)
            .and_then(Value::as_array)
            .filter(|o| !o.is_empty())
        {
            let matches = options
                .iter()
                .any(|option| violations(option, value).is_empty());
            if !matches {
                found.push(format!(
                    "{path}: does not match any of the {} alternatives in {group}",
                    options.len()
                ));
            }
        }
    }

    match value {
        Value::Object(map) => {
            for name in schema
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if !map.contains_key(name) {
                    found.push(format!("{path}: required property {name:?} is missing"));
                }
            }
            let properties = schema.get("properties").and_then(Value::as_object);
            if let Some(properties) = properties {
                for (name, sub) in properties {
                    if let Some(actual) = map.get(name) {
                        check(sub, actual, &format!("{path}.{name}"), found);
                    }
                }
            }
            match schema.get("additionalProperties") {
                Some(Value::Bool(false)) => {
                    for name in map
                        .keys()
                        .filter(|k| !properties.is_some_and(|p| p.contains_key(*k)))
                    {
                        found.push(format!("{path}: property {name:?} is not allowed"));
                    }
                }
                Some(extra @ Value::Object(_)) => {
                    for (name, actual) in map
                        .iter()
                        .filter(|(k, _)| !properties.is_some_and(|p| p.contains_key(*k)))
                    {
                        check(extra, actual, &format!("{path}.{name}"), found);
                    }
                }
                _ => {}
            }
        }
        Value::Array(items) => {
            if let Some(min) = schema.get("minItems").and_then(Value::as_u64) {
                if (items.len() as u64) < min {
                    found.push(format!(
                        "{path}: has {} item(s), at least {min} expected",
                        items.len()
                    ));
                }
            }
            if let Some(max) = schema.get("maxItems").and_then(Value::as_u64) {
                if (items.len() as u64) > max {
                    found.push(format!(
                        "{path}: has {} item(s), at most {max} expected",
                        items.len()
                    ));
                }
            }
            if let Some(sub) = schema.get("items").filter(|s| s.is_object()) {
                for (i, item) in items.iter().enumerate() {
                    check(sub, item, &format!("{path}[{i}]"), found);
                }
            }
        }
        Value::String(text) => {
            let length = text.chars().count() as u64;
            if let Some(min) = schema.get("minLength").and_then(Value::as_u64) {
                if length < min {
                    found.push(format!(
                        "{path}: is {length} character(s) long, at least {min} expected"
                    ));
                }
            }
            if let Some(max) = schema.get("maxLength").and_then(Value::as_u64) {
                if length > max {
                    found.push(format!(
                        "{path}: is {length} character(s) long, at most {max} expected"
                    ));
                }
            }
            if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
                // A pattern this engine cannot read is not a reason to fail a response.
                if Regex::new(pattern).is_ok_and(|re| !re.is_match(text)) {
                    found.push(format!("{path}: does not match the pattern {pattern}"));
                }
            }
        }
        Value::Number(n) => {
            if let Some(actual) = n.as_f64() {
                number_bounds(schema, actual, path, found);
            }
        }
        _ => {}
    }
}

/// `minimum` and `maximum`, each with its exclusive form: a number in OpenAPI 3.1, a flag in 3.0.
fn number_bounds(
    schema: &serde_json::Map<String, Value>,
    actual: f64,
    path: &str,
    found: &mut Vec<String>,
) {
    let bound = |key: &str| schema.get(key).and_then(Value::as_f64);
    let flag = |key: &str| schema.get(key).and_then(Value::as_bool) == Some(true);
    if let Some(min) = bound("minimum") {
        let exclusive = flag("exclusiveMinimum");
        if actual < min || (exclusive && actual == min) {
            found.push(format!(
                "{path}: {actual} is {} {min}",
                if exclusive { "not above" } else { "below" }
            ));
        }
    }
    if let Some(min) = bound("exclusiveMinimum") {
        if actual <= min {
            found.push(format!("{path}: {actual} is not above {min}"));
        }
    }
    if let Some(max) = bound("maximum") {
        let exclusive = flag("exclusiveMaximum");
        if actual > max || (exclusive && actual == max) {
            found.push(format!(
                "{path}: {actual} is {} {max}",
                if exclusive { "not below" } else { "above" }
            ));
        }
    }
    if let Some(max) = bound("exclusiveMaximum") {
        if actual >= max {
            found.push(format!("{path}: {actual} is not below {max}"));
        }
    }
}

/// A value in a message: short, so a large one does not fill it.
fn short(value: &Value) -> String {
    let text = value.to_string();
    if text.chars().count() > 60 {
        format!("{}…", text.chars().take(60).collect::<String>())
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn wrong(schema: Value, value: Value) -> Vec<String> {
        violations(&schema, &value)
    }

    #[test]
    fn a_value_that_fits_has_no_violations() {
        let schema = json!({
            "type": "object",
            "required": ["id", "name"],
            "properties": {
                "id": {"type": "integer"},
                "name": {"type": "string"},
                "tags": {"type": "array", "items": {"type": "string"}},
            },
        });
        assert!(wrong(
            schema,
            json!({"id": 7, "name": "Rex", "tags": ["a", "b"], "extra": true})
        )
        .is_empty());
    }

    #[test]
    fn the_type_is_checked_and_named() {
        assert_eq!(
            wrong(json!({"type": "string"}), json!(5)),
            ["$: expected string, got integer"]
        );
        assert_eq!(
            wrong(json!({"type": "integer"}), json!("5")),
            ["$: expected integer, got string"]
        );
        assert_eq!(
            wrong(json!({"type": "integer"}), json!(5.5)),
            ["$: expected integer, got number"]
        );
        assert_eq!(
            wrong(json!({"type": "boolean"}), json!(null)),
            ["$: expected boolean, got null"]
        );
        assert_eq!(
            wrong(json!({"type": "array"}), json!({})),
            ["$: expected array, got object"]
        );
        assert_eq!(
            wrong(json!({"type": "object"}), json!([])),
            ["$: expected object, got array"]
        );
    }

    #[test]
    fn a_whole_number_is_an_integer_and_an_integer_is_a_number() {
        assert!(wrong(json!({"type": "integer"}), json!(5)).is_empty());
        assert!(wrong(json!({"type": "integer"}), json!(5.0)).is_empty());
        assert!(wrong(json!({"type": "number"}), json!(5)).is_empty());
        assert!(wrong(json!({"type": "number"}), json!(5.5)).is_empty());
    }

    #[test]
    fn a_whole_number_is_named_an_integer_and_any_other_a_number_in_a_message() {
        let string = json!({"type": "string"});
        for whole in [
            json!(5),
            json!(-3),
            json!(0),
            json!(5.0),
            json!(18446744073709551615u64),
        ] {
            assert_eq!(
                wrong(string.clone(), whole.clone()),
                ["$: expected string, got integer"],
                "{whole}"
            );
        }
        for other in [json!(5.5), json!(-0.25)] {
            assert_eq!(
                wrong(string.clone(), other.clone()),
                ["$: expected string, got number"],
                "{other}"
            );
        }
    }

    #[test]
    fn null_is_allowed_by_nullable_and_by_a_list_of_types() {
        assert_eq!(
            wrong(json!({"type": "string"}), json!(null)),
            ["$: expected string, got null"]
        );
        assert!(wrong(json!({"type": "string", "nullable": true}), json!(null)).is_empty());
        assert!(wrong(json!({"type": ["string", "null"]}), json!(null)).is_empty());
        assert_eq!(
            wrong(json!({"type": ["string", "null"]}), json!(3)),
            ["$: expected string or null, got integer"]
        );
        assert_eq!(
            wrong(json!({"type": "string", "nullable": false}), json!(null)),
            ["$: expected string, got null"]
        );
    }

    #[test]
    fn required_properties_are_named_when_missing() {
        let schema = json!({"type": "object", "required": ["id", "name"]});
        assert_eq!(
            wrong(schema, json!({"name": "x"})),
            ["$: required property \"id\" is missing"]
        );
    }

    #[test]
    fn nested_properties_and_items_are_checked_with_their_paths() {
        let schema = json!({
            "type": "object",
            "properties": {
                "owner": {"type": "object", "properties": {"name": {"type": "string"}}},
                "pets": {"type": "array", "items": {"type": "object", "required": ["id"]}},
            },
        });
        let value = json!({"owner": {"name": 3}, "pets": [{"id": 1}, {}, {"id": 2}]});
        assert_eq!(
            wrong(schema, value),
            [
                "$.owner.name: expected string, got integer",
                "$.pets[1]: required property \"id\" is missing"
            ]
        );
    }

    #[test]
    fn a_wrong_type_ends_the_checking_of_that_value() {
        // The value is not an object, so its properties are not gone through.
        let schema = json!({"type": "object", "required": ["id"], "properties": {"id": {"type": "integer"}}});
        assert_eq!(
            wrong(schema, json!("nope")),
            ["$: expected object, got string"]
        );
    }

    #[test]
    fn enums_and_constants_are_checked() {
        assert!(wrong(json!({"enum": ["a", "b"]}), json!("a")).is_empty());
        assert_eq!(
            wrong(json!({"enum": ["a", "b"]}), json!("c")),
            ["$: \"c\" is not one of the allowed values"]
        );
        assert!(wrong(json!({"const": 5}), json!(5)).is_empty());
        assert_eq!(
            wrong(json!({"const": 5}), json!(6)),
            ["$: expected 5, got 6"]
        );
    }

    #[test]
    fn string_lengths_and_patterns_are_checked() {
        let schema =
            json!({"type": "string", "minLength": 2, "maxLength": 4, "pattern": "^[a-z]+$"});
        assert!(wrong(schema.clone(), json!("abc")).is_empty());
        assert_eq!(
            wrong(schema.clone(), json!("a")),
            ["$: is 1 character(s) long, at least 2 expected"]
        );
        assert_eq!(
            wrong(schema.clone(), json!("abcde")),
            ["$: is 5 character(s) long, at most 4 expected"]
        );
        assert_eq!(
            wrong(schema.clone(), json!("AB")),
            ["$: does not match the pattern ^[a-z]+$"]
        );
        // Length is in characters, not bytes.
        assert!(wrong(json!({"maxLength": 2}), json!("éé")).is_empty());
        // A pattern that cannot be read is not held against the response.
        assert!(wrong(json!({"pattern": "("}), json!("anything")).is_empty());
    }

    #[test]
    fn number_bounds_are_checked_in_both_the_3_0_and_the_3_1_forms() {
        let bounded = json!({"minimum": 1, "maximum": 10});
        assert!(wrong(bounded.clone(), json!(1)).is_empty());
        assert!(wrong(bounded.clone(), json!(10)).is_empty());
        assert_eq!(wrong(bounded.clone(), json!(0)), ["$: 0 is below 1"]);
        assert_eq!(wrong(bounded.clone(), json!(11)), ["$: 11 is above 10"]);
        // 3.0: a flag beside the bound.
        let flags = json!({"minimum": 1, "exclusiveMinimum": true, "maximum": 10, "exclusiveMaximum": true});
        assert_eq!(wrong(flags.clone(), json!(1)), ["$: 1 is not above 1"]);
        assert_eq!(wrong(flags.clone(), json!(10)), ["$: 10 is not below 10"]);
        assert!(wrong(flags, json!(5)).is_empty());
        // 3.1: the bound itself.
        let numbers = json!({"exclusiveMinimum": 1, "exclusiveMaximum": 10});
        assert_eq!(wrong(numbers.clone(), json!(1)), ["$: 1 is not above 1"]);
        assert_eq!(wrong(numbers.clone(), json!(10)), ["$: 10 is not below 10"]);
        assert!(wrong(numbers, json!(2)).is_empty());
    }

    #[test]
    fn array_sizes_are_checked() {
        let schema = json!({"type": "array", "minItems": 1, "maxItems": 2});
        assert_eq!(
            wrong(schema.clone(), json!([])),
            ["$: has 0 item(s), at least 1 expected"]
        );
        assert!(
            wrong(schema.clone(), json!([1])).is_empty(),
            "exactly the smallest size is allowed"
        );
        assert!(
            wrong(schema.clone(), json!([1, 2])).is_empty(),
            "and exactly the largest"
        );
        assert_eq!(
            wrong(schema, json!([1, 2, 3])),
            ["$: has 3 item(s), at most 2 expected"]
        );
    }

    #[test]
    fn additional_properties_are_refused_only_when_the_schema_says_false_or_gives_their_schema() {
        let closed =
            json!({"type": "object", "properties": {"a": {}}, "additionalProperties": false});
        assert_eq!(
            wrong(closed.clone(), json!({"a": 1, "b": 2})),
            ["$: property \"b\" is not allowed"]
        );
        assert!(wrong(closed, json!({"a": 1})).is_empty());
        let typed = json!({"type": "object", "additionalProperties": {"type": "integer"}});
        assert_eq!(
            wrong(typed.clone(), json!({"x": "no"})),
            ["$.x: expected integer, got string"]
        );
        assert!(wrong(typed, json!({"x": 1})).is_empty());
        let open = json!({"type": "object", "properties": {"a": {}}});
        assert!(wrong(open, json!({"a": 1, "b": 2})).is_empty());
    }

    #[test]
    fn all_of_needs_every_part_and_any_of_and_one_of_need_one() {
        let all = json!({"allOf": [{"type": "object", "required": ["a"]}, {"type": "object", "required": ["b"]}]});
        assert!(wrong(all.clone(), json!({"a": 1, "b": 2})).is_empty());
        assert_eq!(
            wrong(all, json!({"a": 1})),
            ["$: required property \"b\" is missing"]
        );
        for group in ["anyOf", "oneOf"] {
            let any = json!({ group: [{"type": "string"}, {"type": "integer"}] });
            assert!(wrong(any.clone(), json!("x")).is_empty(), "{group}");
            assert!(wrong(any.clone(), json!(3)).is_empty(), "{group}");
            assert_eq!(
                wrong(any, json!(true)),
                [format!(
                    "$: does not match any of the 2 alternatives in {group}"
                )],
                "{group}"
            );
        }
        // An empty list of alternatives says nothing.
        assert!(wrong(json!({"oneOf": []}), json!(1)).is_empty());
    }

    #[test]
    fn what_it_does_not_know_it_leaves_alone() {
        let schema = json!({"type": "string", "format": "date-time", "readOnly": true, "x-anything": 1, "$ref": "#/components/schemas/Gone", "discriminator": {}});
        assert!(wrong(schema, json!("not a date")).is_empty());
        assert!(wrong(json!({}), json!({"any": "thing"})).is_empty());
        assert!(
            wrong(json!(true), json!(1)).is_empty(),
            "a schema that is not an object says nothing"
        );
        assert!(
            wrong(json!({"type": 5}), json!(1)).is_empty(),
            "a type that is not a name says nothing"
        );
    }

    #[test]
    fn it_stops_at_a_number_of_violations_so_a_report_stays_readable() {
        let schema = json!({"type": "array", "items": {"type": "string"}});
        let value = Value::Array((0..100).map(|n| json!(n)).collect());
        let found = wrong(schema, value);
        assert_eq!(found.len(), MAX_VIOLATIONS);
        assert_eq!(found[0], "$[0]: expected string, got integer");
    }

    #[test]
    fn a_long_value_is_shortened_in_a_message() {
        let long = "x".repeat(200);
        let found = wrong(json!({"enum": ["a"]}), json!(long));
        assert_eq!(found.len(), 1);
        assert!(found[0].contains('…'), "{found:?}");
        assert!(found[0].len() < 150, "{}", found[0].len());
    }
}
