//! Phase 8B — API test cases worked out from an OpenAPI document, by rules, with no AI and no cost.
//!
//! For each operation of a `Spec` (`openapi_spec.rs`) it makes the cases the document itself lets one expect an answer to:
//!
//! - `HAPPY`: a valid request (path values, required query and header parameters, a body built from its schema) that should get one of
//!   the success statuses the document lists and, when it gives one, a body that fits the response's schema;
//! - `NO_AUTH`: the same without the credentials, for an operation the document says needs them: 401 or 403;
//! - `MISSING_REQUIRED`: the valid request without one required query or header parameter, or one required property of the body;
//! - `WRONG_TYPE`: a parameter or body property given a value of the wrong kind (text where a number is due);
//! - `NOT_FOUND`: an id that is not there, for an operation that documents a 404.
//!
//! A case whose answer the document does not promise is not made: the client errors an operation documents are what a case may
//! expect, and an operation that documents none gets no "missing required" case, rather than one that guesses. The request of a
//! case that changes data (POST, PUT, PATCH, DELETE) is derived and shown but marked as not to be run unless the person asked
//! for unsafe methods, so that pointing this at a real API never creates or deletes anything by default.
//!
//! What goes into a request is the smallest that is valid: only what is required, with values taken from the schema's own
//! `example`, `default` or `enum`, else from its type (`sample`).

use crate::execution_plan::{Action, CompilerMethod, ExecutionPlan, ExecutionType};
use crate::openapi_spec::{Location, Operation, Spec};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// The most cases one derivation makes; the rest are left out and the notes say how many.
pub const MAX_CASES: usize = 500;
/// The most cases made for one operation (the happy one counts) unless the person asks for fewer.
pub const DEFAULT_CASES_PER_OPERATION: usize = 8;
pub const MAX_CASES_PER_OPERATION: usize = 20;
const MAX_SAMPLE_DEPTH: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CaseKind {
    Happy,
    NoAuth,
    MissingRequired,
    WrongType,
    NotFound,
}

impl CaseKind {
    pub const ALL: [CaseKind; 5] = [
        CaseKind::Happy,
        CaseKind::NoAuth,
        CaseKind::MissingRequired,
        CaseKind::WrongType,
        CaseKind::NotFound,
    ];

    pub fn name(self) -> &'static str {
        match self {
            CaseKind::Happy => "HAPPY",
            CaseKind::NoAuth => "NO_AUTH",
            CaseKind::MissingRequired => "MISSING_REQUIRED",
            CaseKind::WrongType => "WRONG_TYPE",
            CaseKind::NotFound => "NOT_FOUND",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Options {
    /// Where the API is: an absolute http or https address the paths are added to.
    pub base_url: String,
    /// Which kinds of case to make; all of them when empty.
    #[serde(default)]
    pub kinds: Vec<CaseKind>,
    /// Only these operations (`GET /pets`); all of them when empty.
    #[serde(default)]
    pub operations: Vec<String>,
    /// Let the cases that change data (POST, PUT, PATCH, DELETE) be run.
    #[serde(default)]
    pub include_unsafe: bool,
    /// Headers that carry the credential, sent on every case but `NO_AUTH`. Each value is a reference, `env:NAME`: the value is read
    /// from the orchestrator's environment when the request is made, and neither the plan, the run nor its evidence ever holds it.
    #[serde(default)]
    pub auth_headers: BTreeMap<String, String>,
    #[serde(default)]
    pub max_cases_per_operation: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseRequest {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expect {
    /// The statuses that pass: the response must be one of them.
    pub statuses: Vec<u16>,
    /// A JSON Schema the response body must fit, when the document gives one.
    pub schema: Option<Value>,
    /// Why this is what to expect, in the document's terms.
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Case {
    /// `GET /pets#MISSING_REQUIRED:limit`: unique in a derivation.
    pub id: String,
    pub operation: String,
    pub kind: CaseKind,
    pub title: String,
    pub request: CaseRequest,
    pub expect: Expect,
    /// Why the case is not run, when it is not (a request that changes data, with unsafe methods not allowed).
    pub skipped: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkippedOperation {
    pub operation: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Derived {
    pub cases: Vec<Case>,
    /// Operations no case was made for, and why.
    pub skipped: Vec<SkippedOperation>,
    pub notes: Vec<String>,
}

/// What is wrong with `options`, or `None`.
/// Whether `name` is a header name HTTP allows (a token: letters, digits and ``!#$%&'*+-.^_`|~``). Anything else can only make every
/// request fail when it is built, so it is refused before a run starts.
fn is_header_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

pub fn options_problem(options: &Options) -> Option<String> {
    let base = options.base_url.trim();
    match reqwest::Url::parse(base) {
        Ok(url) if matches!(url.scheme(), "http" | "https") && url.host_str().is_some() => {}
        _ => {
            return Some(format!(
                "base_url must be an absolute http or https address (got {base:?})"
            ))
        }
    }
    for (name, value) in &options.auth_headers {
        if !is_header_name(name) {
            return Some(format!(
                "an authentication header needs a name HTTP allows: letters, digits and - _ . ! # $ % & ' * + ^ ` | ~ (got {name:?})"
            ));
        }
        if crate::execution_plan::secret_name(value).is_none() {
            // A literal value would be stored with the run: only a reference is accepted, and what was typed is never repeated.
            return Some(format!(
                "the value of the header {name} must be a secret reference, env:NAME, where NAME starts with {} (letters, digits and _): the value is read from the orchestrator's environment when a request is made, and is never stored",
                crate::execution_plan::SECRET_NAME_PREFIXES.join(" or ")
            ));
        }
    }
    if let Some(n) = options.max_cases_per_operation {
        if !(1..=MAX_CASES_PER_OPERATION).contains(&n) {
            return Some(format!(
                "max_cases_per_operation must be from 1 to {MAX_CASES_PER_OPERATION}"
            ));
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------------------- sample values

fn format_sample(format: &str) -> Option<&'static str> {
    Some(match format {
        "date" => "2024-01-01",
        "date-time" => "2024-01-01T00:00:00Z",
        "time" => "12:00:00",
        "uuid" => "00000000-0000-4000-8000-000000000000",
        "email" => "user@example.com",
        "uri" | "url" => "https://example.com",
        "hostname" => "example.com",
        "ipv4" => "192.0.2.1",
        "ipv6" => "2001:db8::1",
        "byte" => "AAAA",
        "password" => "Passw0rd!",
        _ => return None,
    })
}

/// The first type a schema names that is not `null`.
fn main_type(schema: &Value) -> Option<String> {
    match schema.get("type")? {
        Value::String(t) => Some(t.clone()),
        Value::Array(list) => list
            .iter()
            .filter_map(Value::as_str)
            .find(|t| *t != "null")
            .map(str::to_string),
        _ => None,
    }
}

/// A value that fits `schema`, as small as it can be: what the document says (`example`, `default`, `const`, the first `enum`
/// entry), else what its type asks for. Only required properties go into an object, but one that lists none is given its first few.
pub fn sample(schema: &Value) -> Value {
    sample_at(schema, 0)
}

fn sample_at(schema: &Value, depth: usize) -> Value {
    let Some(map) = schema.as_object() else {
        return Value::Null;
    };
    if depth > MAX_SAMPLE_DEPTH {
        return Value::Null;
    }
    if let Some(example) = map.get("example") {
        return example.clone();
    }
    if let Some(first) = map
        .get("examples")
        .and_then(Value::as_array)
        .and_then(|e| e.first())
    {
        return first.clone();
    }
    for key in ["default", "const"] {
        if let Some(v) = map.get(key) {
            return v.clone();
        }
    }
    if let Some(first) = map
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|e| e.first())
    {
        return first.clone();
    }
    if let Some(parts) = map.get("allOf").and_then(Value::as_array) {
        let mut merged = Map::new();
        let mut last = Value::Null;
        for part in parts {
            match sample_at(part, depth + 1) {
                Value::Object(m) => merged.extend(m),
                other => last = other,
            }
        }
        return if merged.is_empty() {
            last
        } else {
            Value::Object(merged)
        };
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(first) = map
            .get(key)
            .and_then(Value::as_array)
            .and_then(|a| a.first())
        {
            return sample_at(first, depth + 1);
        }
    }
    let kind = main_type(schema).or_else(|| {
        if map.contains_key("properties") {
            Some("object".to_string())
        } else if map.contains_key("items") {
            Some("array".to_string())
        } else {
            None
        }
    });
    match kind.as_deref() {
        Some("string") => {
            let mut text = map
                .get("format")
                .and_then(Value::as_str)
                .and_then(format_sample)
                .unwrap_or("string")
                .to_string();
            let min = map.get("minLength").and_then(Value::as_u64).unwrap_or(0) as usize;
            while text.chars().count() < min {
                text.push('a');
            }
            if let Some(max) = map.get("maxLength").and_then(Value::as_u64) {
                text = text.chars().take(max as usize).collect();
            }
            Value::String(text)
        }
        Some("integer") => {
            let min = map.get("minimum").and_then(Value::as_i64);
            let exclusive = map.get("exclusiveMinimum").and_then(Value::as_bool) == Some(true);
            let mut value = match (min, map.get("exclusiveMinimum").and_then(Value::as_i64)) {
                (_, Some(bound)) => bound + 1,
                (Some(min), None) => min + i64::from(exclusive),
                (None, None) => 1,
            };
            if let Some(max) = map.get("maximum").and_then(Value::as_i64) {
                value = value.min(max);
            }
            json!(value)
        }
        Some("number") => {
            let min = map.get("minimum").and_then(Value::as_f64);
            json!(min
                .map(|m| m
                    + if map.get("exclusiveMinimum").and_then(Value::as_bool) == Some(true) {
                        1.0
                    } else {
                        0.0
                    })
                .unwrap_or(1.5))
        }
        Some("boolean") => Value::Bool(true),
        Some("array") => {
            let items = map.get("items").cloned().unwrap_or_else(|| json!({}));
            let count = map
                .get("minItems")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .max(1) as usize;
            Value::Array((0..count).map(|_| sample_at(&items, depth + 1)).collect())
        }
        Some("object") => {
            let properties = map.get("properties").and_then(Value::as_object);
            let required: Vec<&str> = map
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let mut object = Map::new();
            if let Some(properties) = properties {
                let chosen: Vec<(&String, &Value)> = if map.contains_key("required") {
                    properties
                        .iter()
                        .filter(|(name, _)| required.contains(&name.as_str()))
                        .collect()
                } else {
                    properties.iter().take(5).collect()
                };
                for (name, sub) in chosen {
                    object.insert(name.clone(), sample_at(sub, depth + 1));
                }
            }
            Value::Object(object)
        }
        _ => Value::Null,
    }
}

/// A value of the wrong kind for `schema`, or `None` when any text would do (a plain string has no wrong value).
pub fn wrong_value(schema: &Value) -> Option<Value> {
    if schema.get("enum").and_then(Value::as_array).is_some() {
        return Some(json!("NOT_A_LISTED_VALUE"));
    }
    match main_type(schema)?.as_str() {
        "integer" | "number" => Some(json!("not-a-number")),
        "boolean" => Some(json!("not-a-boolean")),
        "array" | "object" => Some(json!("not-a-structure")),
        _ => None,
    }
}

/// A value a request would carry in a path, a query or a header, as text.
fn as_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// `text` for the inside of a path segment: every byte but a letter, a digit or `-._~` is written `%XX`.
pub fn encode_segment(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------------- statuses

fn documented_statuses(operation: &Operation) -> Vec<u16> {
    let mut statuses: Vec<u16> = operation
        .responses
        .iter()
        .filter_map(|r| r.status.parse::<u16>().ok())
        .collect();
    statuses.sort_unstable();
    statuses
}

/// What a success may be: the 2xx statuses the operation lists; the usual four when it lists none of them by number (`2XX`,
/// `default`, or nothing at all).
fn success_statuses(operation: &Operation) -> (Vec<u16>, String) {
    let listed: Vec<u16> = documented_statuses(operation)
        .into_iter()
        .filter(|s| (200..300).contains(s))
        .collect();
    if listed.is_empty() {
        let why = if operation.responses.is_empty() {
            "the document lists no responses, so any usual success status is accepted"
        } else {
            "the document lists no numbered success status, so any usual one is accepted"
        };
        (vec![200, 201, 202, 204], why.to_string())
    } else {
        let names: Vec<String> = listed.iter().map(u16::to_string).collect();
        (
            listed,
            format!(
                "the document lists {} as the success response",
                names.join(" and ")
            ),
        )
    }
}

/// The 4xx statuses a case of `kind` may get: those the operation documents that suit it (`preferred`, in order), else any
/// 4xx it documents. `None` when it documents no client error at all: there is then nothing promised to test.
fn client_errors(operation: &Operation, preferred: &[u16]) -> Option<Vec<u16>> {
    let documented: Vec<u16> = documented_statuses(operation)
        .into_iter()
        .filter(|s| (400..500).contains(s))
        .collect();
    if documented.is_empty() {
        return None;
    }
    let suiting: Vec<u16> = preferred
        .iter()
        .copied()
        .filter(|s| documented.contains(s))
        .collect();
    Some(if suiting.is_empty() {
        documented
    } else {
        suiting
    })
}

fn schema_of_success(operation: &Operation) -> Option<Value> {
    let mut listed: Vec<&crate::openapi_spec::ResponseSpec> = operation
        .responses
        .iter()
        .filter(|r| {
            r.status
                .parse::<u16>()
                .is_ok_and(|s| (200..300).contains(&s))
        })
        .collect();
    listed.sort_by(|a, b| a.status.cmp(&b.status));
    listed
        .into_iter()
        .find(|r| {
            r.content_type
                .as_deref()
                .is_some_and(|ct| ct.contains("json"))
        })
        .and_then(|r| r.schema.clone())
        .filter(|s| s.as_object().is_some_and(|o| !o.is_empty()))
}

// ---------------------------------------------------------------------------------------------------------- one request

/// The pieces of a valid request, kept apart so that a case can take one away or spoil one.
#[derive(Debug, Clone)]
struct Draft {
    path_values: BTreeMap<String, String>,
    query: Vec<(String, String)>,
    headers: BTreeMap<String, String>,
    body: Option<Value>,
}

fn valid_draft(operation: &Operation) -> Draft {
    let mut draft = Draft {
        path_values: BTreeMap::new(),
        query: Vec::new(),
        headers: BTreeMap::new(),
        body: None,
    };
    for parameter in operation.parameters.iter().filter(|p| p.required) {
        let value = sample(&parameter.schema);
        match parameter.location {
            Location::Path => {
                draft
                    .path_values
                    .insert(parameter.name.clone(), as_text(&value));
            }
            Location::Query => match value {
                Value::Array(items) => draft
                    .query
                    .extend(items.iter().map(|i| (parameter.name.clone(), as_text(i)))),
                other => draft.query.push((parameter.name.clone(), as_text(&other))),
            },
            Location::Header => {
                draft
                    .headers
                    .insert(parameter.name.clone(), as_text(&value));
            }
            Location::Cookie => {}
        }
    }
    if let Some(body) = operation
        .body
        .as_ref()
        .filter(|b| b.content_type.contains("json"))
    {
        draft.body = Some(sample(&body.schema));
    }
    draft
}

fn build_request(
    operation: &Operation,
    draft: &Draft,
    base_url: &str,
    auth: &BTreeMap<String, String>,
) -> Result<CaseRequest, String> {
    let mut path = operation.path.clone();
    for name in operation
        .path
        .split('{')
        .skip(1)
        .filter_map(|rest| rest.split('}').next())
    {
        let value = draft.path_values.get(name).ok_or_else(|| {
            format!("the path parameter {name} is not described by the operation")
        })?;
        path = path.replace(&format!("{{{name}}}"), &encode_segment(value));
    }
    let mut url = reqwest::Url::parse(&format!("{}{path}", base_url.trim().trim_end_matches('/')))
        .map_err(|e| format!("cannot build the address: {e}"))?;
    if !draft.query.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for (name, value) in &draft.query {
            pairs.append_pair(name, value);
        }
    }
    let mut headers = draft.headers.clone();
    if schema_of_success(operation).is_some() {
        headers
            .entry("Accept".to_string())
            .or_insert_with(|| "application/json".to_string());
    }
    headers.extend(auth.iter().map(|(k, v)| (k.clone(), v.clone())));
    Ok(CaseRequest {
        method: operation.method.clone(),
        url: url.to_string(),
        headers,
        body: draft.body.clone(),
    })
}

fn is_unsafe(method: &str) -> bool {
    !matches!(method, "GET" | "HEAD" | "OPTIONS")
}

/// Whether the document gives a value for something to be tried with (`example`, a first of `examples`, `default`, `const`, or an
/// `enum` with an entry), rather than leaving it to be made up from the type.
fn gives_a_value(schema: &Value) -> bool {
    let Some(map) = schema.as_object() else {
        return false;
    };
    let listed = |key: &str| {
        map.get(key)
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty())
    };
    map.contains_key("example")
        || map.contains_key("default")
        || map.contains_key("const")
        || listed("examples")
        || listed("enum")
}

/// Whether a request for the operation has to be filled in with a path value the document does not give: it is then made up, and the API
/// may have nothing by that name.
fn has_made_up_path_value(operation: &Operation) -> bool {
    operation
        .parameters
        .iter()
        .any(|p| p.location == Location::Path && !gives_a_value(&p.schema))
}

/// The first few of some names, and how many more there are.
fn some_of(names: &[String], most: usize) -> String {
    let shown = names
        .iter()
        .take(most)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    match names.len().saturating_sub(most) {
        0 => shown,
        more => format!("{shown} and {more} more"),
    }
}

// ---------------------------------------------------------------------------------------------------------- deriving

struct Maker<'a> {
    operation: &'a Operation,
    options: &'a Options,
    kinds: &'a [CaseKind],
    cases: Vec<Case>,
    notes: Vec<String>,
    limit: usize,
}

impl Maker<'_> {
    fn wants(&self, kind: CaseKind) -> bool {
        self.kinds.contains(&kind)
    }

    fn room(&self) -> bool {
        self.cases.len() < self.limit
    }

    fn push(
        &mut self,
        kind: CaseKind,
        detail: Option<&str>,
        title: String,
        request: CaseRequest,
        expect: Expect,
    ) {
        let id = match detail {
            Some(d) => format!("{}#{}:{d}", self.operation.id, kind.name()),
            None => format!("{}#{}", self.operation.id, kind.name()),
        };
        let skipped =
            (is_unsafe(&self.operation.method) && !self.options.include_unsafe).then(|| {
                format!(
                    "{} changes data on the server: it is run only when unsafe methods are allowed",
                    self.operation.method
                )
            });
        self.cases.push(Case {
            id,
            operation: self.operation.id.clone(),
            kind,
            title,
            request,
            expect,
            skipped,
        });
    }
}

/// The names of the properties a JSON body requires, and the schemas of its properties.
fn body_properties(operation: &Operation) -> Option<(Vec<String>, &Map<String, Value>)> {
    let body = operation
        .body
        .as_ref()
        .filter(|b| b.content_type.contains("json"))?;
    let properties = body.schema.get("properties")?.as_object()?;
    let required = body
        .schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    Some((required, properties))
}

fn derive_operation(
    operation: &Operation,
    options: &Options,
    kinds: &[CaseKind],
    limit: usize,
) -> Result<(Vec<Case>, Vec<String>), String> {
    let base = options.base_url.as_str();
    let draft = valid_draft(operation);
    let auth = &options.auth_headers;
    let happy_request = build_request(operation, &draft, base, auth)?;
    let mut maker = Maker {
        operation,
        options,
        kinds,
        cases: Vec::new(),
        notes: Vec::new(),
        limit,
    };
    let title = |what: &str| format!("{}: {what}", operation.id);

    if maker.wants(CaseKind::Happy) {
        let (statuses, why) = success_statuses(operation);
        maker.push(
            CaseKind::Happy,
            None,
            title("a valid request gets a documented success"),
            happy_request.clone(),
            Expect {
                statuses,
                schema: schema_of_success(operation),
                why,
            },
        );
    }

    if maker.wants(CaseKind::NoAuth) && operation.requires_auth && maker.room() {
        let request = build_request(operation, &draft, base, &BTreeMap::new())?;
        maker.push(
            CaseKind::NoAuth,
            None,
            title("without a credential it is refused"),
            request,
            Expect {
                statuses: vec![401, 403],
                schema: None,
                why: "the document says this operation needs a credential".to_string(),
            },
        );
    }

    let validation_statuses = client_errors(operation, &[400, 422]);
    if maker.wants(CaseKind::MissingRequired) {
        match &validation_statuses {
            None => maker.notes.push(format!("{}: no client-error response is documented, so no \"missing required\" case is made", operation.id)),
            Some(statuses) => {
                for parameter in operation.parameters.iter().filter(|p| p.required && matches!(p.location, Location::Query | Location::Header)) {
                    if !maker.room() {
                        break;
                    }
                    let mut spoiled = draft.clone();
                    spoiled.query.retain(|(name, _)| name != &parameter.name);
                    spoiled.headers.remove(&parameter.name);
                    let request = build_request(operation, &spoiled, base, auth)?;
                    let place = if parameter.location == Location::Query { "query parameter" } else { "header" };
                    maker.push(
                        CaseKind::MissingRequired,
                        Some(&parameter.name),
                        title(&format!("without the required {place} {}", parameter.name)),
                        request,
                        Expect { statuses: statuses.clone(), schema: None, why: "the document documents a client error for a bad request".to_string() },
                    );
                }
                if let Some((required, _)) = body_properties(operation) {
                    for name in required {
                        if !maker.room() {
                            break;
                        }
                        let mut spoiled = draft.clone();
                        if let Some(Value::Object(body)) = spoiled.body.as_mut() {
                            body.remove(&name);
                        }
                        let request = build_request(operation, &spoiled, base, auth)?;
                        maker.push(
                            CaseKind::MissingRequired,
                            Some(&format!("body.{name}")),
                            title(&format!("without the required body property {name}")),
                            request,
                            Expect { statuses: statuses.clone(), schema: None, why: "the document documents a client error for a bad request".to_string() },
                        );
                    }
                }
            }
        }
    }

    if maker.wants(CaseKind::WrongType) {
        if let Some(statuses) = validation_statuses.clone() {
            let mut made = 0;
            for parameter in &operation.parameters {
                if made >= 2 || !maker.room() {
                    break;
                }
                let Some(wrong) = wrong_value(&parameter.schema) else {
                    continue;
                };
                let mut spoiled = draft.clone();
                let mut allowed = statuses.clone();
                match parameter.location {
                    Location::Path => {
                        spoiled
                            .path_values
                            .insert(parameter.name.clone(), as_text(&wrong));
                        // A path that names nothing is also a 404.
                        if documented_statuses(operation).contains(&404) && !allowed.contains(&404)
                        {
                            allowed.push(404);
                        }
                    }
                    Location::Query => {
                        spoiled.query.retain(|(name, _)| name != &parameter.name);
                        spoiled
                            .query
                            .push((parameter.name.clone(), as_text(&wrong)));
                    }
                    Location::Header => {
                        spoiled
                            .headers
                            .insert(parameter.name.clone(), as_text(&wrong));
                    }
                    Location::Cookie => continue,
                }
                let request = build_request(operation, &spoiled, base, auth)?;
                made += 1;
                maker.push(
                    CaseKind::WrongType,
                    Some(&parameter.name),
                    title(&format!(
                        "the parameter {} with a value of the wrong kind",
                        parameter.name
                    )),
                    request,
                    Expect {
                        statuses: allowed,
                        schema: None,
                        why: "the document documents a client error for a bad request".to_string(),
                    },
                );
            }
            if let Some((_, properties)) = body_properties(operation) {
                for (name, schema) in properties {
                    if made >= 2 || !maker.room() {
                        break;
                    }
                    let Some(wrong) = wrong_value(schema) else {
                        continue;
                    };
                    let mut spoiled = draft.clone();
                    if let Some(Value::Object(body)) = spoiled.body.as_mut() {
                        body.insert(name.clone(), wrong);
                    }
                    let request = build_request(operation, &spoiled, base, auth)?;
                    made += 1;
                    maker.push(
                        CaseKind::WrongType,
                        Some(&format!("body.{name}")),
                        title(&format!(
                            "the body property {name} with a value of the wrong kind"
                        )),
                        request,
                        Expect {
                            statuses: statuses.clone(),
                            schema: None,
                            why: "the document documents a client error for a bad request"
                                .to_string(),
                        },
                    );
                }
            }
        }
    }

    if maker.wants(CaseKind::NotFound)
        && maker.room()
        && documented_statuses(operation).contains(&404)
    {
        if let Some(parameter) = operation
            .parameters
            .iter()
            .rev()
            .find(|p| p.location == Location::Path)
        {
            let mut spoiled = draft.clone();
            let missing = match main_type(&parameter.schema).as_deref() {
                Some("integer") | Some("number") => "999999999".to_string(),
                _ if parameter.schema.get("format").and_then(Value::as_str) == Some("uuid") => {
                    "ffffffff-ffff-4fff-8fff-ffffffffffff".to_string()
                }
                _ => "aiqa-no-such-id".to_string(),
            };
            spoiled.path_values.insert(parameter.name.clone(), missing);
            let request = build_request(operation, &spoiled, base, auth)?;
            maker.push(
                CaseKind::NotFound,
                Some(&parameter.name),
                title(&format!("a {} that is not there is a 404", parameter.name)),
                request,
                Expect {
                    statuses: vec![404],
                    schema: None,
                    why: "the document lists 404 as a response".to_string(),
                },
            );
        }
    }
    Ok((maker.cases, maker.notes))
}

/// The cases the document lets one expect an answer to, for the operations asked for.
pub fn derive(spec: &Spec, options: &Options) -> Derived {
    let kinds: Vec<CaseKind> = if options.kinds.is_empty() {
        CaseKind::ALL.to_vec()
    } else {
        options.kinds.clone()
    };
    let limit = options
        .max_cases_per_operation
        .unwrap_or(DEFAULT_CASES_PER_OPERATION)
        .clamp(1, MAX_CASES_PER_OPERATION);
    let mut derived = Derived {
        cases: Vec::new(),
        skipped: Vec::new(),
        notes: spec.notes.clone(),
    };
    let mut made_up: Vec<String> = Vec::new();
    for operation in &spec.operations {
        if !options.operations.is_empty() && !options.operations.contains(&operation.id) {
            continue;
        }
        if operation.deprecated {
            derived.skipped.push(SkippedOperation {
                operation: operation.id.clone(),
                reason: "the operation is marked deprecated".to_string(),
            });
            continue;
        }
        match derive_operation(operation, options, &kinds, limit) {
            Ok((cases, notes)) => {
                derived.notes.extend(notes);
                // A case whose own values are the point (a wrong type, an id that is not there) does not depend on the made-up one.
                if has_made_up_path_value(operation)
                    && cases.iter().any(|c| {
                        matches!(
                            c.kind,
                            CaseKind::Happy | CaseKind::NoAuth | CaseKind::MissingRequired
                        )
                    })
                {
                    made_up.push(operation.id.clone());
                }
                if cases.is_empty() {
                    derived.skipped.push(SkippedOperation {
                        operation: operation.id.clone(),
                        reason: "none of the asked-for kinds of case applies to it".to_string(),
                    });
                }
                derived.cases.extend(cases);
            }
            Err(reason) => derived.skipped.push(SkippedOperation {
                operation: operation.id.clone(),
                reason,
            }),
        }
    }
    if !made_up.is_empty() {
        let (who, has, their) = if made_up.len() == 1 {
            (made_up[0].clone(), "has", "its requests use")
        } else {
            (
                format!("{} operations ({})", made_up.len(), some_of(&made_up, 3)),
                "have",
                "their requests use",
            )
        };
        derived.notes.push(format!("{who} {has} a path parameter that the document gives no example, default or enum for, so {their} a made-up value: a 404 there may only mean that the API has no such thing (give the parameter an `example` in the document to test with one that exists)"));
    }
    if derived.cases.len() > MAX_CASES {
        let left_out = derived.cases.len() - MAX_CASES;
        derived.cases.truncate(MAX_CASES);
        derived.notes.push(format!("{left_out} case(s) beyond the first {MAX_CASES} are left out: ask for fewer operations or kinds"));
    }
    derived
}

/// The plan a case runs as: its request, then what it expects of the answer.
pub fn to_plan(case: &Case, task_id: &str) -> ExecutionPlan {
    let mut actions = vec![Action::Request {
        method: case.request.method.clone(),
        url: case.request.url.clone(),
        headers: case.request.headers.clone(),
        body: case.request.body.clone(),
    }];
    actions.push(Action::AssertStatusIn {
        statuses: case.expect.statuses.clone(),
    });
    if let Some(schema) = &case.expect.schema {
        actions.push(Action::AssertBodySchema {
            schema: schema.clone(),
        });
    }
    ExecutionPlan {
        test_case_id: task_id.to_string(),
        execution_type: ExecutionType::Api,
        target: Some(case.request.url.clone()),
        actions,
        compiler: CompilerMethod::Deterministic,
        diagnostics: vec![format!(
            "Derived from the OpenAPI document by rule ({}): {}",
            case.kind.name(),
            case.expect.why
        )],
        provider: None,
        model: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi_spec::parse;

    // ------------------------------------------------------------------------------------------------- sample values

    #[test]
    fn a_string_is_made_from_its_format_and_kept_to_its_length() {
        assert_eq!(sample(&json!({"type": "string"})), json!("string"));
        assert_eq!(
            sample(&json!({"type": "string", "format": "date"})),
            json!("2024-01-01")
        );
        assert_eq!(
            sample(&json!({"type": "string", "format": "date-time"})),
            json!("2024-01-01T00:00:00Z")
        );
        assert_eq!(
            sample(&json!({"type": "string", "format": "uuid"})),
            json!("00000000-0000-4000-8000-000000000000")
        );
        assert_eq!(
            sample(&json!({"type": "string", "format": "email"})),
            json!("user@example.com")
        );
        assert_eq!(
            sample(&json!({"type": "string", "format": "uri"})),
            json!("https://example.com")
        );
        assert_eq!(
            sample(&json!({"type": "string", "format": "something-else"})),
            json!("string")
        );
        assert_eq!(
            sample(&json!({"type": "string", "minLength": 9})),
            json!("stringaaa")
        );
        assert_eq!(
            sample(&json!({"type": "string", "minLength": 3})),
            json!("string")
        );
        assert_eq!(
            sample(&json!({"type": "string", "maxLength": 3})),
            json!("str")
        );
    }

    #[test]
    fn a_number_is_the_smallest_the_bounds_allow() {
        assert_eq!(sample(&json!({"type": "integer"})), json!(1));
        assert_eq!(sample(&json!({"type": "integer", "minimum": 5})), json!(5));
        assert_eq!(
            sample(&json!({"type": "integer", "minimum": 5, "exclusiveMinimum": true})),
            json!(6)
        );
        assert_eq!(
            sample(&json!({"type": "integer", "exclusiveMinimum": 5})),
            json!(6)
        );
        assert_eq!(sample(&json!({"type": "integer", "maximum": 0})), json!(0));
        assert_eq!(
            sample(&json!({"type": "integer", "minimum": -3})),
            json!(-3)
        );
        assert_eq!(sample(&json!({"type": "number"})), json!(1.5));
        assert_eq!(sample(&json!({"type": "number", "minimum": 2})), json!(2.0));
        assert_eq!(
            sample(&json!({"type": "number", "minimum": 2, "exclusiveMinimum": true})),
            json!(3.0)
        );
        assert_eq!(sample(&json!({"type": "boolean"})), json!(true));
    }

    #[test]
    fn what_the_document_says_comes_before_what_the_type_would_give() {
        assert_eq!(
            sample(&json!({"type": "string", "example": "Rex", "default": "d", "enum": ["e"]})),
            json!("Rex")
        );
        assert_eq!(
            sample(&json!({"type": "string", "examples": ["first", "second"]})),
            json!("first")
        );
        assert_eq!(
            sample(&json!({"type": "string", "default": "d", "enum": ["e"]})),
            json!("d")
        );
        assert_eq!(
            sample(&json!({"type": "string", "const": "only"})),
            json!("only")
        );
        assert_eq!(
            sample(&json!({"type": "integer", "enum": [7, 8]})),
            json!(7)
        );
    }

    #[test]
    fn an_array_has_at_least_one_item_and_the_minimum_the_schema_asks_for() {
        assert_eq!(
            sample(&json!({"type": "array", "items": {"type": "integer"}})),
            json!([1])
        );
        assert_eq!(
            sample(&json!({"type": "array", "minItems": 3, "items": {"type": "boolean"}})),
            json!([true, true, true])
        );
        assert_eq!(
            sample(&json!({"items": {"type": "string"}})),
            json!(["string"])
        );
    }

    #[test]
    fn an_object_has_its_required_properties_or_else_its_first_few() {
        let required = json!({"type": "object", "required": ["b"], "properties": {"a": {"type": "integer"}, "b": {"type": "string"}}});
        assert_eq!(sample(&required), json!({"b": "string"}));
        let unlisted = json!({"type": "object", "properties": {"a": {"type": "integer"}, "b": {"type": "string"}}});
        assert_eq!(sample(&unlisted), json!({"a": 1, "b": "string"}));
        let many = json!({"properties": {"a": {}, "b": {}, "c": {}, "d": {}, "e": {}, "f": {}}});
        assert_eq!(
            sample(&many).as_object().unwrap().len(),
            5,
            "an object that lists none required is given its first five"
        );
        assert_eq!(
            sample(
                &json!({"type": "object", "required": [], "properties": {"a": {"type": "integer"}}})
            ),
            json!({})
        );
    }

    #[test]
    fn nested_objects_are_made_all_the_way_down_and_a_loop_is_cut() {
        let nested = json!({"type": "object", "required": ["inner"], "properties": {"inner": {"type": "object", "required": ["n"], "properties": {"n": {"type": "integer"}}}}});
        assert_eq!(sample(&nested), json!({"inner": {"n": 1}}));
        let mut deep = json!({"type": "integer"});
        for _ in 0..12 {
            deep = json!({"type": "object", "required": ["x"], "properties": {"x": deep}});
        }
        let mut node = sample(&deep);
        let mut levels = 0;
        while let Some(next) = node.get("x").cloned() {
            node = next;
            levels += 1;
        }
        assert!(levels < 12, "cut short after {levels} levels");
        assert_eq!(node, Value::Null);
    }

    #[test]
    fn all_of_is_merged_and_one_of_and_any_of_take_the_first() {
        let all = json!({"allOf": [{"type": "object", "required": ["a"], "properties": {"a": {"type": "integer"}}}, {"type": "object", "required": ["b"], "properties": {"b": {"type": "string"}}}]});
        assert_eq!(sample(&all), json!({"a": 1, "b": "string"}));
        assert_eq!(
            sample(&json!({"oneOf": [{"type": "integer"}, {"type": "string"}]})),
            json!(1)
        );
        assert_eq!(
            sample(&json!({"anyOf": [{"type": "boolean"}]})),
            json!(true)
        );
        assert_eq!(
            sample(&json!({"allOf": [{"type": "string"}]})),
            json!("string")
        );
    }

    #[test]
    fn a_list_of_types_gives_the_first_that_is_not_null_and_nothing_known_gives_null() {
        assert_eq!(sample(&json!({"type": ["null", "integer"]})), json!(1));
        assert_eq!(sample(&json!({})), Value::Null);
        assert_eq!(sample(&json!("not a schema")), Value::Null);
        assert_eq!(sample(&json!({"type": 5})), Value::Null);
    }

    #[test]
    fn a_wrong_value_is_of_another_kind_and_a_plain_string_has_none() {
        assert_eq!(
            wrong_value(&json!({"type": "integer"})),
            Some(json!("not-a-number"))
        );
        assert_eq!(
            wrong_value(&json!({"type": "number"})),
            Some(json!("not-a-number"))
        );
        assert_eq!(
            wrong_value(&json!({"type": "boolean"})),
            Some(json!("not-a-boolean"))
        );
        assert_eq!(
            wrong_value(&json!({"type": "array"})),
            Some(json!("not-a-structure"))
        );
        assert_eq!(
            wrong_value(&json!({"type": "object"})),
            Some(json!("not-a-structure"))
        );
        assert_eq!(
            wrong_value(&json!({"type": "string", "enum": ["a"]})),
            Some(json!("NOT_A_LISTED_VALUE"))
        );
        assert_eq!(wrong_value(&json!({"type": "string"})), None);
        assert_eq!(wrong_value(&json!({})), None);
    }

    #[test]
    fn a_value_in_a_path_is_written_so_it_cannot_change_the_path() {
        assert_eq!(encode_segment("abc-DEF_1.2~3"), "abc-DEF_1.2~3");
        assert_eq!(encode_segment("a b/c?d#e"), "a%20b%2Fc%3Fd%23e");
        assert_eq!(encode_segment("é"), "%C3%A9");
        assert_eq!(encode_segment(""), "");
    }

    // ---------------------------------------------------------------------------------------------------- options

    fn options() -> Options {
        Options {
            base_url: "https://api.example.test/v1/".to_string(),
            kinds: Vec::new(),
            operations: Vec::new(),
            include_unsafe: false,
            auth_headers: BTreeMap::from([(
                "Authorization".to_string(),
                "env:AIQA_SECRET_TOKEN".to_string(),
            )]),
            max_cases_per_operation: None,
        }
    }

    #[test]
    fn good_options_have_no_problem() {
        assert_eq!(options_problem(&options()), None);
    }

    #[test]
    fn options_are_refused_in_words() {
        let bad_base = |base: &str| {
            options_problem(&Options {
                base_url: base.to_string(),
                ..options()
            })
        };
        assert!(bad_base("").unwrap().contains("absolute http or https"));
        assert!(bad_base("/relative")
            .unwrap()
            .contains("absolute http or https"));
        assert!(bad_base("ftp://files.example.test")
            .unwrap()
            .contains("absolute http or https"));
        assert!(bad_base("not a url")
            .unwrap()
            .contains("absolute http or https"));
        assert_eq!(bad_base("http://localhost:8080"), None);

        let with_header = |name: &str, value: &str| {
            options_problem(&Options {
                auth_headers: BTreeMap::from([(name.to_string(), value.to_string())]),
                ..options()
            })
        };
        assert!(with_header("", "x").unwrap().contains("needs a name"));
        for bad in [
            "Bad Name",
            "Bad:Name",
            "Bad(Name)",
            "X-Ünï",
            "Tab\tName",
            "a,b",
            "a/b",
            "quote\"d",
        ] {
            let problem = with_header(bad, "env:AIQA_SECRET_KEY").unwrap_or_default();
            assert!(
                problem.contains("needs a name HTTP allows"),
                "{bad:?}: {problem}"
            );
        }
        for good in [
            "Authorization",
            "X-Api-Key",
            "api_key",
            "x.y",
            "A1",
            "X~Token",
            "Upper-and-lower",
        ] {
            assert_eq!(with_header(good, "env:AIQA_SECRET_KEY"), None, "{good}");
        }
        assert!(with_header("Authorization", "env:GROQ_API_KEY")
            .unwrap()
            .contains("secret reference"));
        assert!(
            !with_header("Authorization", "env:GROQ_API_KEY")
                .unwrap()
                .contains("GROQ_API_KEY"),
            "the variable asked for is not repeated"
        );
        let literal = with_header("X-Api-Key", "a-literal-key-value").unwrap();
        assert!(literal.contains("must be a secret reference"), "{literal}");
        assert!(
            !literal.contains("a-literal-key-value"),
            "a value typed by mistake is not repeated: {literal}"
        );
        assert_eq!(with_header("X-Api-Key", "env:AIQA_LOGIN_KEY"), None);
        assert_eq!(with_header("X-Api-Key", "env:AIQA_SECRET_KEY"), None);

        let cases = |n: usize| {
            options_problem(&Options {
                max_cases_per_operation: Some(n),
                ..options()
            })
        };
        assert!(cases(0).unwrap().contains("from 1 to 20"));
        assert!(cases(21).unwrap().contains("from 1 to 20"));
        assert_eq!(cases(1), None);
        assert_eq!(cases(20), None);
    }

    #[test]
    fn options_read_from_json_with_the_kinds_by_their_wire_names() {
        let parsed: Options = serde_json::from_value(json!({"base_url": "https://x.test", "kinds": ["HAPPY", "NO_AUTH", "MISSING_REQUIRED", "WRONG_TYPE", "NOT_FOUND"]})).unwrap();
        assert_eq!(parsed.kinds, CaseKind::ALL.to_vec());
        assert!(!parsed.include_unsafe);
        assert!(parsed.auth_headers.is_empty());
        for kind in CaseKind::ALL {
            assert_eq!(serde_json::to_value(kind).unwrap(), json!(kind.name()));
        }
    }

    // ---------------------------------------------------------------------------------------------------- deriving

    fn petstore() -> Value {
        json!({
            "openapi": "3.0.3",
            "info": {"title": "Pets", "version": "1"},
            "security": [{"key": []}],
            "paths": {
                "/pets": {
                    "get": {
                        "parameters": [
                            {"name": "limit", "in": "query", "required": true, "schema": {"type": "integer", "minimum": 1}},
                            {"name": "tag", "in": "query", "schema": {"type": "string"}},
                        ],
                        "responses": {
                            "200": {"description": "ok", "content": {"application/json": {"schema": {"type": "array", "items": {"type": "object", "required": ["id"], "properties": {"id": {"type": "integer"}}}}}}},
                            "400": {"description": "bad"},
                        },
                    },
                    "post": {
                        "security": [],
                        "requestBody": {"required": true, "content": {"application/json": {"schema": {"type": "object", "required": ["name", "age"], "properties": {"name": {"type": "string"}, "age": {"type": "integer"}, "note": {"type": "string"}}}}}},
                        "responses": {"201": {"description": "created"}, "422": {"description": "invalid"}},
                    },
                },
                "/pets/{petId}": {
                    "get": {
                        "parameters": [
                            {"name": "petId", "in": "path", "required": true, "schema": {"type": "integer"}},
                            {"name": "X-Trace", "in": "header", "required": true, "schema": {"type": "string", "enum": ["on", "off"]}},
                        ],
                        "responses": {
                            "200": {"description": "ok", "content": {"application/json": {"schema": {"type": "object", "required": ["id"], "properties": {"id": {"type": "integer"}}}}}},
                            "400": {"description": "bad"},
                            "404": {"description": "none"},
                        },
                    },
                    "delete": {
                        "parameters": [{"name": "petId", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}],
                        "responses": {"204": {"description": "gone"}, "404": {"description": "none"}},
                    },
                },
                "/old": {"get": {"deprecated": true, "responses": {"200": {"description": "ok"}}}},
            },
        })
    }

    fn derived(options: &Options) -> Derived {
        derive(&parse(&petstore()).unwrap(), options)
    }

    fn ids(derived: &Derived) -> Vec<&str> {
        derived.cases.iter().map(|c| c.id.as_str()).collect()
    }

    fn case<'a>(derived: &'a Derived, id: &str) -> &'a Case {
        derived
            .cases
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("no case {id} in {:?}", ids(derived)))
    }

    #[test]
    fn every_kind_of_case_is_made_for_each_operation_that_promises_an_answer_to_it() {
        let d = derived(&options());
        assert_eq!(
            ids(&d),
            [
                "GET /pets#HAPPY",
                "GET /pets#NO_AUTH",
                "GET /pets#MISSING_REQUIRED:limit",
                "GET /pets#WRONG_TYPE:limit",
                "POST /pets#HAPPY",
                "POST /pets#MISSING_REQUIRED:body.name",
                "POST /pets#MISSING_REQUIRED:body.age",
                "POST /pets#WRONG_TYPE:body.age",
                "GET /pets/{petId}#HAPPY",
                "GET /pets/{petId}#NO_AUTH",
                "GET /pets/{petId}#MISSING_REQUIRED:X-Trace",
                "GET /pets/{petId}#WRONG_TYPE:petId",
                "GET /pets/{petId}#WRONG_TYPE:X-Trace",
                "GET /pets/{petId}#NOT_FOUND:petId",
                "DELETE /pets/{petId}#HAPPY",
                "DELETE /pets/{petId}#NO_AUTH",
                "DELETE /pets/{petId}#NOT_FOUND:petId",
            ]
        );
        assert_eq!(
            d.skipped,
            [SkippedOperation {
                operation: "GET /old".to_string(),
                reason: "the operation is marked deprecated".to_string()
            }]
        );
    }

    #[test]
    fn a_valid_request_has_only_what_is_required_and_expects_a_documented_success_and_its_schema() {
        let d = derived(&options());
        let happy = case(&d, "GET /pets#HAPPY");
        assert_eq!(happy.request.method, "GET");
        assert_eq!(
            happy.request.url,
            "https://api.example.test/v1/pets?limit=1"
        );
        assert_eq!(
            happy.request.headers,
            BTreeMap::from([
                ("Accept".to_string(), "application/json".to_string()),
                (
                    "Authorization".to_string(),
                    "env:AIQA_SECRET_TOKEN".to_string()
                )
            ])
        );
        assert_eq!(happy.request.body, None);
        assert_eq!(happy.expect.statuses, [200]);
        assert_eq!(happy.expect.schema.as_ref().unwrap()["type"], "array");
        assert_eq!(
            happy.expect.why,
            "the document lists 200 as the success response"
        );
        assert_eq!(
            happy.title,
            "GET /pets: a valid request gets a documented success"
        );
        assert_eq!(happy.skipped, None);

        let path = case(&d, "GET /pets/{petId}#HAPPY");
        assert_eq!(path.request.url, "https://api.example.test/v1/pets/1");
        assert_eq!(
            path.request.headers.get("X-Trace").map(String::as_str),
            Some("on"),
            "a required header takes its first enum value"
        );
    }

    #[test]
    fn a_body_is_built_from_its_schema_and_a_request_that_changes_data_is_not_run_unless_allowed() {
        let d = derived(&options());
        let post = case(&d, "POST /pets#HAPPY");
        assert_eq!(post.request.body, Some(json!({"age": 1, "name": "string"})));
        assert_eq!(post.expect.statuses, [201]);
        assert_eq!(
            post.expect.schema, None,
            "the document gives the 201 no schema"
        );
        assert_eq!(
            post.skipped.as_deref(),
            Some("POST changes data on the server: it is run only when unsafe methods are allowed")
        );
        assert!(case(&d, "DELETE /pets/{petId}#HAPPY").skipped.is_some());
        assert!(case(&d, "GET /pets#HAPPY").skipped.is_none());

        let allowed = derived(&Options {
            include_unsafe: true,
            ..options()
        });
        assert!(allowed.cases.iter().all(|c| c.skipped.is_none()));
    }

    #[test]
    fn without_a_credential_is_made_only_where_the_document_asks_for_one() {
        let d = derived(&options());
        let none = case(&d, "GET /pets#NO_AUTH");
        assert!(!none.request.headers.contains_key("Authorization"));
        assert_eq!(none.request.url, "https://api.example.test/v1/pets?limit=1");
        assert_eq!(none.expect.statuses, [401, 403]);
        assert!(
            d.cases.iter().all(|c| c.id != "POST /pets#NO_AUTH"),
            "security: [] wants none"
        );
    }

    #[test]
    fn a_missing_required_case_leaves_out_exactly_one_thing_and_expects_the_documented_client_error(
    ) {
        let d = derived(&options());
        let query = case(&d, "GET /pets#MISSING_REQUIRED:limit");
        assert_eq!(query.request.url, "https://api.example.test/v1/pets");
        assert_eq!(query.expect.statuses, [400]);
        assert_eq!(
            query.title,
            "GET /pets: without the required query parameter limit"
        );
        let header = case(&d, "GET /pets/{petId}#MISSING_REQUIRED:X-Trace");
        assert!(!header.request.headers.contains_key("X-Trace"));
        assert_eq!(header.request.url, "https://api.example.test/v1/pets/1");
        assert_eq!(
            header.title,
            "GET /pets/{petId}: without the required header X-Trace"
        );
        let body = case(&d, "POST /pets#MISSING_REQUIRED:body.name");
        assert_eq!(body.request.body, Some(json!({"age": 1})));
        assert_eq!(
            body.expect.statuses,
            [422],
            "the document documents 422, not 400"
        );
        assert_eq!(
            body.title,
            "POST /pets: without the required body property name"
        );
        assert_eq!(
            case(&d, "POST /pets#MISSING_REQUIRED:body.age")
                .request
                .body,
            Some(json!({"name": "string"}))
        );
    }

    #[test]
    fn a_wrong_type_case_spoils_one_value_and_a_bad_path_may_also_be_a_404() {
        let d = derived(&options());
        assert_eq!(
            case(&d, "GET /pets#WRONG_TYPE:limit").request.url,
            "https://api.example.test/v1/pets?limit=not-a-number"
        );
        assert_eq!(
            case(&d, "GET /pets#WRONG_TYPE:limit").expect.statuses,
            [400]
        );
        let path = case(&d, "GET /pets/{petId}#WRONG_TYPE:petId");
        assert_eq!(
            path.request.url,
            "https://api.example.test/v1/pets/not-a-number"
        );
        assert_eq!(path.expect.statuses, [400, 404]);
        let header = case(&d, "GET /pets/{petId}#WRONG_TYPE:X-Trace");
        assert_eq!(
            header.request.headers.get("X-Trace").map(String::as_str),
            Some("NOT_A_LISTED_VALUE")
        );
        assert_eq!(header.expect.statuses, [400]);
        assert_eq!(
            case(&d, "POST /pets#WRONG_TYPE:body.age").request.body,
            Some(json!({"age": "not-a-number", "name": "string"}))
        );
    }

    #[test]
    fn a_bad_path_lists_404_once_when_it_is_also_the_only_client_error_documented() {
        let doc = json!({"openapi": "3.0.0", "paths": {"/things/{id}": {"get": {
            "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "integer"}}],
            "responses": {"200": {"description": "ok"}, "404": {"description": "none"}},
        }}}});
        let d = derive(
            &parse(&doc).unwrap(),
            &Options {
                auth_headers: BTreeMap::new(),
                ..options()
            },
        );
        assert_eq!(
            case(&d, "GET /things/{id}#WRONG_TYPE:id").expect.statuses,
            [404]
        );
    }

    #[test]
    fn a_not_found_case_uses_an_id_that_is_not_there_of_the_kind_the_parameter_has() {
        let d = derived(&options());
        let number = case(&d, "GET /pets/{petId}#NOT_FOUND:petId");
        assert_eq!(
            number.request.url,
            "https://api.example.test/v1/pets/999999999"
        );
        assert_eq!(number.expect.statuses, [404]);
        let uuid = case(&d, "DELETE /pets/{petId}#NOT_FOUND:petId");
        assert_eq!(
            uuid.request.url,
            "https://api.example.test/v1/pets/ffffffff-ffff-4fff-8fff-ffffffffffff"
        );
        assert_ne!(
            uuid.request.url,
            case(&d, "DELETE /pets/{petId}#HAPPY").request.url
        );
    }

    #[test]
    fn an_operation_that_documents_no_client_error_gets_no_negative_case_and_the_notes_say_so() {
        let mut doc = petstore();
        doc["paths"]["/pets"]["get"]["responses"]
            .as_object_mut()
            .unwrap()
            .remove("400");
        let d = derive(&parse(&doc).unwrap(), &options());
        assert!(d.cases.iter().all(
            |c| !c.id.starts_with("GET /pets#MISSING") && !c.id.starts_with("GET /pets#WRONG")
        ));
        assert!(d.cases.iter().any(|c| c.id == "GET /pets#HAPPY"));
        assert!(
            d.notes
                .iter()
                .any(|n| n.starts_with("GET /pets: no client-error response is documented")),
            "{:?}",
            d.notes
        );
    }

    #[test]
    fn when_the_document_lists_no_numbered_success_any_usual_one_is_accepted_and_says_so() {
        let doc = json!({"openapi": "3.0.0", "paths": {"/a": {"get": {"responses": {"default": {"description": "x"}}}}, "/b": {"get": {}}}});
        let d = derive(&parse(&doc).unwrap(), &options());
        let a = case(&d, "GET /a#HAPPY");
        assert_eq!(a.expect.statuses, [200, 201, 202, 204]);
        assert_eq!(
            a.expect.why,
            "the document lists no numbered success status, so any usual one is accepted"
        );
        assert_eq!(
            case(&d, "GET /b#HAPPY").expect.why,
            "the document lists no responses, so any usual success status is accepted"
        );
    }

    #[test]
    fn several_documented_successes_are_all_accepted_and_the_lowest_one_gives_the_schema() {
        let doc = json!({"openapi": "3.0.0", "paths": {"/a": {"get": {"responses": {
            "201": {"description": "c", "content": {"application/json": {"schema": {"type": "string"}}}},
            "200": {"description": "o", "content": {"application/json": {"schema": {"type": "integer"}}}},
        }}}}});
        let d = derive(&parse(&doc).unwrap(), &options());
        let happy = case(&d, "GET /a#HAPPY");
        assert_eq!(happy.expect.statuses, [200, 201]);
        assert_eq!(happy.expect.schema, Some(json!({"type": "integer"})));
        assert_eq!(
            happy.expect.why,
            "the document lists 200 and 201 as the success response"
        );
    }

    #[test]
    fn a_response_that_is_not_json_or_has_an_empty_schema_gives_no_schema_to_check() {
        let doc = json!({"openapi": "3.0.0", "paths": {"/a": {"get": {"responses": {"200": {"description": "x", "content": {"text/plain": {"schema": {"type": "string"}}}}}}}, "/b": {"get": {"responses": {"200": {"description": "x", "content": {"application/json": {"schema": {}}}}}}}}});
        let d = derive(&parse(&doc).unwrap(), &options());
        assert_eq!(case(&d, "GET /a#HAPPY").expect.schema, None);
        assert_eq!(case(&d, "GET /b#HAPPY").expect.schema, None);
        assert!(
            !case(&d, "GET /a#HAPPY")
                .request
                .headers
                .contains_key("Accept"),
            "no JSON is expected, so none is asked for"
        );
    }

    #[test]
    fn the_kinds_asked_for_are_the_ones_made_and_operations_can_be_picked() {
        let only_happy = derived(&Options {
            kinds: vec![CaseKind::Happy],
            ..options()
        });
        assert_eq!(
            ids(&only_happy),
            [
                "GET /pets#HAPPY",
                "POST /pets#HAPPY",
                "GET /pets/{petId}#HAPPY",
                "DELETE /pets/{petId}#HAPPY"
            ]
        );
        let negatives = derived(&Options {
            kinds: vec![CaseKind::NoAuth, CaseKind::NotFound],
            operations: vec!["GET /pets/{petId}".to_string()],
            ..options()
        });
        assert_eq!(
            ids(&negatives),
            [
                "GET /pets/{petId}#NO_AUTH",
                "GET /pets/{petId}#NOT_FOUND:petId"
            ]
        );
        assert!(
            negatives.skipped.is_empty(),
            "an operation that was not asked for is not reported as skipped"
        );
    }

    #[test]
    fn an_operation_none_of_the_kinds_applies_to_is_reported_as_skipped() {
        let d = derived(&Options {
            kinds: vec![CaseKind::NotFound],
            operations: vec!["GET /pets".to_string()],
            ..options()
        });
        assert!(d.cases.is_empty());
        assert_eq!(
            d.skipped,
            [SkippedOperation {
                operation: "GET /pets".to_string(),
                reason: "none of the asked-for kinds of case applies to it".to_string()
            }]
        );
    }

    #[test]
    fn no_operation_gets_more_cases_than_the_limit_and_the_happy_one_comes_first() {
        let two = derived(&Options {
            max_cases_per_operation: Some(2),
            ..options()
        });
        let of = |op: &str| {
            two.cases
                .iter()
                .filter(|c| c.operation == op)
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(of("GET /pets"), ["GET /pets#HAPPY", "GET /pets#NO_AUTH"]);
        assert_eq!(
            of("GET /pets/{petId}"),
            ["GET /pets/{petId}#HAPPY", "GET /pets/{petId}#NO_AUTH"]
        );
        let one = derived(&Options {
            max_cases_per_operation: Some(1),
            ..options()
        });
        assert!(one.cases.iter().all(|c| c.kind == CaseKind::Happy));
    }

    #[test]
    fn an_operation_whose_path_has_an_undescribed_parameter_is_skipped_with_the_reason() {
        let doc = json!({"openapi": "3.0.0", "paths": {"/things/{id}": {"get": {"responses": {"200": {"description": "ok"}}}}}});
        let d = derive(&parse(&doc).unwrap(), &options());
        assert!(d.cases.is_empty());
        assert_eq!(
            d.skipped[0].reason,
            "the path parameter id is not described by the operation"
        );
    }

    #[test]
    fn no_credential_headers_are_sent_when_none_were_given() {
        let d = derived(&Options {
            auth_headers: BTreeMap::new(),
            ..options()
        });
        assert!(d
            .cases
            .iter()
            .all(|c| !c.request.headers.contains_key("Authorization")));
    }

    #[test]
    fn required_query_values_are_written_into_the_address_and_an_array_repeats_its_name() {
        let doc = json!({"openapi": "3.0.0", "paths": {"/s": {"get": {"parameters": [
            {"name": "q", "in": "query", "required": true, "schema": {"type": "string", "example": "a b&c"}},
            {"name": "ids", "in": "query", "required": true, "schema": {"type": "array", "minItems": 2, "items": {"type": "integer"}}},
        ], "responses": {"200": {"description": "ok"}}}}}});
        let d = derive(
            &parse(&doc).unwrap(),
            &Options {
                auth_headers: BTreeMap::new(),
                ..options()
            },
        );
        assert_eq!(
            case(&d, "GET /s#HAPPY").request.url,
            "https://api.example.test/v1/s?q=a+b%26c&ids=1&ids=1"
        );
    }

    #[test]
    fn a_path_value_is_encoded_into_the_address() {
        let doc = json!({"openapi": "3.0.0", "paths": {"/files/{name}": {"get": {"parameters": [{"name": "name", "in": "path", "required": true, "schema": {"type": "string", "example": "my file/1"}}], "responses": {"200": {"description": "ok"}}}}}});
        let d = derive(
            &parse(&doc).unwrap(),
            &Options {
                auth_headers: BTreeMap::new(),
                ..options()
            },
        );
        assert_eq!(
            case(&d, "GET /files/{name}#HAPPY").request.url,
            "https://api.example.test/v1/files/my%20file%2F1"
        );
    }

    #[test]
    fn a_path_value_the_document_does_not_give_is_made_up_and_one_note_says_so() {
        let d = derived(&options());
        let notes: Vec<&String> = d
            .notes
            .iter()
            .filter(|n| n.contains("made-up value"))
            .collect();
        assert_eq!(notes.len(), 1, "one note for all of them: {:?}", d.notes);
        assert!(notes[0].starts_with("2 operations ("), "{}", notes[0]);
        assert!(
            notes[0].contains("GET /pets/{petId}") && notes[0].contains("DELETE /pets/{petId}")
        );
        assert!(notes[0].contains(") have a path parameter that the document gives no example, default or enum for, so their requests use a made-up value: a 404 there may only mean that the API has no such thing"), "{}", notes[0]);
        assert!(notes[0].ends_with(
            "(give the parameter an `example` in the document to test with one that exists)"
        ));
    }

    #[test]
    fn one_operation_with_a_made_up_path_value_is_named_in_the_singular() {
        let d = derived(&Options {
            operations: vec!["GET /pets/{petId}".to_string()],
            ..options()
        });
        let note = d
            .notes
            .iter()
            .find(|n| n.contains("made-up value"))
            .expect("a note");
        assert!(note.starts_with("GET /pets/{petId} has a path parameter that the document gives no example, default or enum for, so its requests use a made-up value:"), "{note}");
    }

    #[test]
    fn no_note_when_the_document_gives_the_path_value_or_the_cases_do_not_use_it() {
        let with = |parameter: Value| {
            let doc = json!({"openapi": "3.0.0", "paths": {"/things/{id}": {"get": {"parameters": [parameter], "responses": {"200": {"description": "ok"}, "404": {"description": "none"}}}}}});
            derive(
                &parse(&doc).unwrap(),
                &Options {
                    auth_headers: BTreeMap::new(),
                    ..options()
                },
            )
        };
        let noted = |d: &Derived| d.notes.iter().any(|n| n.contains("made-up value"));
        let id =
            |schema: Value| json!({"name": "id", "in": "path", "required": true, "schema": schema});
        assert!(noted(&with(id(json!({"type": "string"})))));
        assert!(
            noted(&with(json!({"name": "id", "in": "path", "required": true}))),
            "no schema at all"
        );
        for given in [
            json!({"type": "string", "example": "abc"}),
            json!({"type": "string", "examples": ["abc"]}),
            json!({"type": "string", "default": "abc"}),
            json!({"type": "string", "const": "abc"}),
            json!({"type": "string", "enum": ["abc", "def"]}),
        ] {
            assert!(!noted(&with(id(given.clone()))), "{given}");
        }
        assert!(
            !noted(&with(
                json!({"name": "id", "in": "path", "required": true, "example": "abc", "schema": {"type": "string"}})
            )),
            "an example beside the schema counts"
        );
        // An empty list gives nothing to try.
        assert!(noted(&with(id(json!({"type": "string", "enum": []})))));
        assert!(noted(&with(id(json!({"type": "string", "examples": []})))));
        // A case with an id of its own does not depend on the made-up one.
        let own = derive(
            &parse(&petstore()).unwrap(),
            &Options {
                kinds: vec![CaseKind::NotFound],
                ..options()
            },
        );
        assert!(!own.cases.is_empty() && !noted(&own), "{:?}", own.notes);
    }

    #[test]
    fn an_example_beside_the_schema_is_what_the_request_is_built_with() {
        let doc = json!({"openapi": "3.0.0", "paths": {
            "/things/{id}": {"get": {"parameters": [
                {"name": "id", "in": "path", "required": true, "example": "t-77", "schema": {"type": "string"}},
                {"name": "q", "in": "query", "required": true, "examples": {"a": {"value": "find me"}}, "schema": {"type": "string"}},
            ], "responses": {"200": {"description": "ok"}}}},
            "/things": {"post": {"requestBody": {"required": true, "content": {"application/json": {"example": {"name": "Rex", "age": 3}, "schema": {"type": "object", "required": ["name"], "properties": {"name": {"type": "string"}}}}}}, "responses": {"201": {"description": "made"}}}},
        }});
        let d = derive(
            &parse(&doc).unwrap(),
            &Options {
                auth_headers: BTreeMap::new(),
                include_unsafe: true,
                ..options()
            },
        );
        assert_eq!(
            case(&d, "GET /things/{id}#HAPPY").request.url,
            "https://api.example.test/v1/things/t-77?q=find+me"
        );
        assert_eq!(
            case(&d, "POST /things#HAPPY").request.body,
            Some(json!({"name": "Rex", "age": 3})),
            "the body the document shows, not one made from its schema"
        );
        assert!(
            !d.notes.iter().any(|n| n.contains("made-up value")),
            "{:?}",
            d.notes
        );
    }

    #[test]
    fn a_body_that_is_not_json_is_not_derived() {
        let doc = json!({"openapi": "3.0.0", "paths": {"/x": {"post": {"requestBody": {"content": {"application/xml": {"schema": {"type": "string"}}}}, "responses": {"200": {"description": "ok"}}}}}});
        let d = derive(
            &parse(&doc).unwrap(),
            &Options {
                include_unsafe: true,
                ..options()
            },
        );
        assert_eq!(case(&d, "POST /x#HAPPY").request.body, None);
    }

    #[test]
    fn a_derivation_is_the_same_every_time() {
        assert_eq!(derived(&options()), derived(&options()));
    }

    #[test]
    fn there_is_a_limit_to_how_many_cases_one_derivation_makes_and_the_notes_say_how_many_were_left_out(
    ) {
        // 300 operations that each want a credential: a happy case and a no-credential case for every one, 600 in all.
        let mut paths = Map::new();
        for n in 0..crate::openapi_spec::MAX_OPERATIONS {
            paths.insert(
                format!("/p{n:04}"),
                json!({"get": {"responses": {"200": {"description": "ok"}}}}),
            );
        }
        let doc =
            json!({"openapi": "3.0.0", "security": [{"k": []}], "paths": Value::Object(paths)});
        let d = derive(&parse(&doc).unwrap(), &options());
        assert_eq!(d.cases.len(), MAX_CASES);
        let left_out = 2 * crate::openapi_spec::MAX_OPERATIONS - MAX_CASES;
        assert!(d.notes.iter().any(|n| n == &format!("{left_out} case(s) beyond the first {MAX_CASES} are left out: ask for fewer operations or kinds")), "{:?}", d.notes);
    }

    // -------------------------------------------------------------------------------------------------------- plans

    #[test]
    fn a_case_becomes_a_plan_with_its_request_and_what_it_expects() {
        let d = derived(&options());
        let plan = to_plan(case(&d, "GET /pets#HAPPY"), "task-1");
        assert_eq!(plan.test_case_id, "task-1");
        assert_eq!(plan.execution_type, ExecutionType::Api);
        assert_eq!(
            plan.target.as_deref(),
            Some("https://api.example.test/v1/pets?limit=1")
        );
        assert_eq!(plan.compiler, CompilerMethod::Deterministic);
        assert_eq!(plan.actions.len(), 3);
        assert!(
            matches!(&plan.actions[0], Action::Request { method, url, .. } if method == "GET" && url == "https://api.example.test/v1/pets?limit=1")
        );
        assert_eq!(
            plan.actions[1],
            Action::AssertStatusIn {
                statuses: vec![200]
            }
        );
        assert!(
            matches!(&plan.actions[2], Action::AssertBodySchema { schema } if schema["type"] == "array")
        );
        assert_eq!(plan.diagnostics, ["Derived from the OpenAPI document by rule (HAPPY): the document lists 200 as the success response"]);
    }

    #[test]
    fn a_case_without_a_schema_has_no_schema_assertion() {
        let d = derived(&options());
        let plan = to_plan(case(&d, "GET /pets#NO_AUTH"), "t");
        assert_eq!(plan.actions.len(), 2);
        assert_eq!(
            plan.actions[1],
            Action::AssertStatusIn {
                statuses: vec![401, 403]
            }
        );
    }

    #[test]
    fn the_credential_stays_a_reference_all_the_way_into_the_plan() {
        let d = derived(&options());
        let plan = to_plan(case(&d, "GET /pets#HAPPY"), "t");
        let Action::Request { headers, .. } = &plan.actions[0] else {
            panic!("no request")
        };
        assert_eq!(
            headers.get("Authorization").map(String::as_str),
            Some("env:AIQA_SECRET_TOKEN")
        );
    }
}
