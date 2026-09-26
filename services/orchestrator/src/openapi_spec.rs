//! Phase 8B — what an OpenAPI 3.x or Swagger 2.0 document says about each of its operations, in one shape.
//!
//! `parse` reads the document (already JSON: `tools::schema_inspector::parse_openapi_document` turns YAML into it) into a list of
//! operations: method and path, the parameters and the request body it takes, the responses it documents and whether it wants a
//! credential. Every `$ref` on the way is resolved (`resolve`), to a depth, so that a schema that refers to itself is cut short
//! rather than followed for ever; a reference to somewhere outside the document is read as an empty schema and said so in the notes.
//! Swagger 2.0 is read into the same shape (a `body` parameter becomes the request body, a response's `schema` its schema), so
//! what derives test cases from it (`openapi_cases.rs`) knows one model, not two.

use serde::Serialize;
use serde_json::{Map, Value};

/// How many operations a document may have before the rest are left out (and the notes say so).
pub const MAX_OPERATIONS: usize = 300;
/// How deep a `$ref` chain and a schema may go before it is cut short.
pub const MAX_DEPTH: usize = 12;

const METHODS: [&str; 7] = ["get", "put", "post", "delete", "patch", "head", "options"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Location {
    Path,
    Query,
    Header,
    Cookie,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Parameter {
    pub name: String,
    pub location: Location,
    pub required: bool,
    /// The schema of the value, references resolved; `{}` when the document gives none.
    pub schema: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Body {
    pub content_type: String,
    pub required: bool,
    pub schema: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResponseSpec {
    /// As written: `200`, `404`, `4XX`, `default`.
    pub status: String,
    pub content_type: Option<String>,
    pub schema: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Operation {
    /// `GET /pets/{petId}`: unique in a document.
    pub id: String,
    pub method: String,
    pub path: String,
    pub operation_id: Option<String>,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub deprecated: bool,
    pub parameters: Vec<Parameter>,
    pub body: Option<Body>,
    pub responses: Vec<ResponseSpec>,
    /// The operation wants a credential (its `security`, or the document's, lists a scheme, and does not also allow none).
    pub requires_auth: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Spec {
    pub title: String,
    pub version: String,
    /// The base addresses the document gives, server variables replaced by their defaults; may be relative (`/api/v3`).
    pub servers: Vec<String>,
    pub operations: Vec<Operation>,
    /// What to know about how the document was read: what was left out and why.
    pub notes: Vec<String>,
}

/// The value a `$ref` such as `#/components/schemas/Pet` points at, or `None` when it points outside the document or nowhere.
fn lookup<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    let pointer = reference.strip_prefix('#')?;
    if pointer.is_empty() {
        return Some(root);
    }
    let mut node = root;
    for part in pointer.strip_prefix('/')?.split('/') {
        let key = part.replace("~1", "/").replace("~0", "~");
        node = match node {
            Value::Object(map) => map.get(&key)?,
            Value::Array(items) => items.get(key.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(node)
}

/// `value` with every `$ref` in it replaced by what it points at, however deep, to `MAX_DEPTH` levels of reference: a schema that
/// refers to itself is followed that far and then cut short with `{}`. Keys written beside a `$ref` are kept, over the target's.
/// A reference that points outside the document, or nowhere, becomes `{}` and is named in `unresolved`.
pub fn resolve(root: &Value, value: &Value, unresolved: &mut Vec<String>) -> Value {
    resolve_at(root, value, 0, unresolved)
}

fn resolve_at(root: &Value, value: &Value, depth: usize, unresolved: &mut Vec<String>) -> Value {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(reference)) = map.get("$ref") {
                if depth >= MAX_DEPTH {
                    return Value::Object(Map::new());
                }
                let Some(target) = lookup(root, reference) else {
                    if !unresolved.contains(reference) {
                        unresolved.push(reference.clone());
                    }
                    return Value::Object(Map::new());
                };
                let mut merged = match resolve_at(root, target, depth + 1, unresolved) {
                    Value::Object(m) => m,
                    other => return other,
                };
                for (key, sibling) in map.iter().filter(|(k, _)| k.as_str() != "$ref") {
                    merged.insert(key.clone(), resolve_at(root, sibling, depth, unresolved));
                }
                return Value::Object(merged);
            }
            Value::Object(
                map.iter()
                    .map(|(k, v)| (k.clone(), resolve_at(root, v, depth, unresolved)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| resolve_at(root, v, depth, unresolved))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
}

/// The base addresses of a document: OpenAPI 3 `servers` (with `{variable}`s given their default), Swagger 2 `schemes`, `host`, `basePath`.
fn servers_of(root: &Value) -> Vec<String> {
    let mut servers = Vec::new();
    if let Some(list) = root.get("servers").and_then(Value::as_array) {
        for server in list {
            let Some(mut url) = text(server, "url") else {
                continue;
            };
            if let Some(vars) = server.get("variables").and_then(Value::as_object) {
                for (name, var) in vars {
                    if let Some(default) = var.get("default").and_then(Value::as_str) {
                        url = url.replace(&format!("{{{name}}}"), default);
                    }
                }
            }
            servers.push(url);
        }
    } else if let Some(host) = text(root, "host") {
        let scheme = root
            .get("schemes")
            .and_then(Value::as_array)
            .and_then(|s| {
                s.iter()
                    .filter_map(Value::as_str)
                    .find(|s| *s == "https")
                    .or_else(|| s.first().and_then(Value::as_str))
            })
            .unwrap_or("https");
        let base = text(root, "basePath").unwrap_or_default();
        servers.push(format!("{scheme}://{host}{}", base.trim_end_matches('/')));
    } else if let Some(base) = text(root, "basePath") {
        servers.push(base);
    }
    servers
}

/// A Swagger 2.0 parameter carries its type beside `name` and `in`, not under `schema`: the schema it means.
fn swagger_parameter_schema(parameter: &Value) -> Value {
    if let Some(schema) = parameter.get("schema") {
        return schema.clone();
    }
    let mut schema = Map::new();
    for key in [
        "type",
        "format",
        "items",
        "enum",
        "default",
        "minimum",
        "maximum",
        "minLength",
        "maxLength",
        "pattern",
        "example",
    ] {
        if let Some(v) = parameter.get(key) {
            schema.insert(key.to_string(), v.clone());
        }
    }
    Value::Object(schema)
}

fn location_of(name: &str) -> Option<Location> {
    match name {
        "path" => Some(Location::Path),
        "query" => Some(Location::Query),
        "header" => Some(Location::Header),
        "cookie" => Some(Location::Cookie),
        _ => None,
    }
}

/// The parameters of an operation: those of its path item, overridden by its own (same name and place), references resolved.
fn parameters_of(
    root: &Value,
    path_item: &Value,
    operation: &Value,
    swagger: bool,
    unresolved: &mut Vec<String>,
) -> (Vec<Parameter>, Option<Body>) {
    let mut parameters: Vec<Parameter> = Vec::new();
    let mut body = None;
    for source in [path_item, operation] {
        for raw in source
            .get("parameters")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let parameter = resolve(root, raw, unresolved);
            let Some(name) = text(&parameter, "name") else {
                continue;
            };
            let place = parameter
                .get("in")
                .and_then(Value::as_str)
                .unwrap_or("query");
            let required = parameter.get("required").and_then(Value::as_bool) == Some(true);
            if swagger && place == "body" {
                let content_type = operation
                    .get("consumes")
                    .or_else(|| root.get("consumes"))
                    .and_then(Value::as_array)
                    .and_then(|c| c.first())
                    .and_then(Value::as_str)
                    .unwrap_or("application/json")
                    .to_string();
                body = Some(Body {
                    content_type,
                    required,
                    schema: parameter
                        .get("schema")
                        .cloned()
                        .unwrap_or_else(|| Value::Object(Map::new())),
                });
                continue;
            }
            let Some(location) = location_of(place) else {
                continue;
            };
            let schema = if swagger {
                swagger_parameter_schema(&parameter)
            } else {
                parameter
                    .get("schema")
                    .cloned()
                    .unwrap_or_else(|| Value::Object(Map::new()))
            };
            let parameter = Parameter {
                name,
                location,
                required: required || location == Location::Path,
                schema,
            };
            match parameters
                .iter_mut()
                .find(|p| p.name == parameter.name && p.location == parameter.location)
            {
                Some(existing) => *existing = parameter,
                None => parameters.push(parameter),
            }
        }
    }
    (parameters, body)
}

/// The request body of an OpenAPI 3 operation: JSON when it offers JSON, else its first content type.
fn body_of(root: &Value, operation: &Value, unresolved: &mut Vec<String>) -> Option<Body> {
    let request_body = resolve(root, operation.get("requestBody")?, unresolved);
    let content = request_body.get("content").and_then(Value::as_object)?;
    let (content_type, media) = content
        .iter()
        .find(|(ct, _)| ct.contains("json"))
        .or_else(|| content.iter().next())?;
    Some(Body {
        content_type: content_type.clone(),
        required: request_body.get("required").and_then(Value::as_bool) == Some(true),
        schema: media
            .get("schema")
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new())),
    })
}

fn responses_of(
    root: &Value,
    operation: &Value,
    swagger: bool,
    unresolved: &mut Vec<String>,
) -> Vec<ResponseSpec> {
    let Some(responses) = operation.get("responses").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut list: Vec<ResponseSpec> = responses
        .iter()
        .map(|(status, raw)| {
            let response = resolve(root, raw, unresolved);
            let (content_type, schema) = if swagger {
                (
                    operation
                        .get("produces")
                        .or_else(|| root.get("produces"))
                        .and_then(Value::as_array)
                        .and_then(|p| p.first())
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    response.get("schema").cloned(),
                )
            } else {
                match response.get("content").and_then(Value::as_object) {
                    Some(content) => {
                        let picked = content
                            .iter()
                            .find(|(ct, _)| ct.contains("json"))
                            .or_else(|| content.iter().next());
                        (
                            picked.map(|(ct, _)| ct.clone()),
                            picked.and_then(|(_, m)| m.get("schema").cloned()),
                        )
                    }
                    None => (None, None),
                }
            };
            ResponseSpec {
                status: status.clone(),
                content_type,
                schema,
            }
        })
        .collect();
    list.sort_by(|a, b| a.status.cmp(&b.status));
    list
}

/// Whether a `security` list wants a credential: it names a scheme, and no alternative in it is the empty requirement (anonymous).
fn wants_credentials(security: &Value) -> bool {
    let Some(list) = security.as_array() else {
        return false;
    };
    !list.is_empty()
        && !list
            .iter()
            .any(|requirement| requirement.as_object().is_some_and(Map::is_empty))
}

/// Reads a document into its operations. Refuses what is not an OpenAPI or Swagger document, or has no paths.
pub fn parse(document: &Value) -> Result<Spec, String> {
    let swagger = document.get("swagger").is_some();
    if !swagger && document.get("openapi").is_none() {
        return Err("this is not an OpenAPI or Swagger document (it has neither an `openapi` nor a `swagger` field)".to_string());
    }
    let paths = document
        .get("paths")
        .and_then(Value::as_object)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| "the document has no paths, so there is nothing to test".to_string())?;

    let info = document.get("info");
    let mut notes = Vec::new();
    let mut unresolved = Vec::new();
    let mut operations = Vec::new();
    'paths: for (path, item) in paths {
        for method in METHODS {
            let Some(operation) = item.get(method) else {
                continue;
            };
            if operations.len() >= MAX_OPERATIONS {
                notes.push(format!("only the first {MAX_OPERATIONS} operations are read; the rest of the document is left out"));
                break 'paths;
            }
            // Every `$ref` on the way is resolved as the pieces are read, so none is left in what comes out.
            let (parameters, swagger_body) =
                parameters_of(document, item, operation, swagger, &mut unresolved);
            let body = if swagger {
                swagger_body
            } else {
                body_of(document, operation, &mut unresolved)
            };
            let responses = responses_of(document, operation, swagger, &mut unresolved);
            let security = operation
                .get("security")
                .or_else(|| document.get("security"))
                .cloned()
                .unwrap_or(Value::Null);
            operations.push(Operation {
                id: format!("{} {}", method.to_uppercase(), path),
                method: method.to_uppercase(),
                path: path.clone(),
                operation_id: text(operation, "operationId"),
                summary: text(operation, "summary"),
                tags: operation
                    .get("tags")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                deprecated: operation.get("deprecated").and_then(Value::as_bool) == Some(true),
                parameters,
                body,
                responses,
                requires_auth: wants_credentials(&security),
            });
        }
    }
    if operations.is_empty() {
        return Err("the document has paths but no operation (no get, post, put, delete, patch, head or options) on them".to_string());
    }
    if !unresolved.is_empty() {
        notes.push(format!(
            "{} reference(s) point outside the document or nowhere and were read as an empty schema: {}",
            unresolved.len(),
            unresolved.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(Spec {
        title: info
            .and_then(|i| text(i, "title"))
            .unwrap_or_else(|| "Untitled API".to_string()),
        version: info
            .and_then(|i| text(i, "version"))
            .unwrap_or_else(|| "unknown".to_string()),
        servers: servers_of(document),
        operations,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pets() -> Value {
        json!({
            "openapi": "3.0.3",
            "info": {"title": "Pet store", "version": "1.2.0"},
            "servers": [{"url": "https://{env}.example.test/api/v3", "variables": {"env": {"default": "petstore"}}}],
            "security": [{"key": []}],
            "paths": {
                "/pets": {
                    "parameters": [{"name": "trace", "in": "header", "schema": {"type": "string"}}],
                    "get": {
                        "operationId": "listPets", "summary": "List pets", "tags": ["pets"],
                        "parameters": [
                            {"name": "limit", "in": "query", "required": true, "schema": {"type": "integer", "minimum": 1}},
                            {"$ref": "#/components/parameters/Status"},
                        ],
                        "responses": {
                            "200": {"description": "ok", "content": {"application/json": {"schema": {"type": "array", "items": {"$ref": "#/components/schemas/Pet"}}}}},
                            "400": {"$ref": "#/components/responses/BadRequest"},
                        },
                    },
                    "post": {
                        "security": [],
                        "requestBody": {"required": true, "content": {"application/xml": {"schema": {"type": "string"}}, "application/json": {"schema": {"$ref": "#/components/schemas/Pet"}}}},
                        "responses": {"201": {"description": "created"}},
                    },
                },
                "/pets/{petId}": {
                    "get": {
                        "parameters": [{"name": "petId", "in": "path", "schema": {"type": "integer"}}],
                        "deprecated": true,
                        "security": [{"key": []}, {}],
                        "responses": {"200": {"description": "ok"}, "404": {"description": "none"}},
                    },
                },
            },
            "components": {
                "schemas": {"Pet": {"type": "object", "required": ["name"], "properties": {"name": {"type": "string"}, "friend": {"$ref": "#/components/schemas/Pet"}}}},
                "parameters": {"Status": {"name": "status", "in": "query", "schema": {"type": "string", "enum": ["a", "b"]}}},
                "responses": {"BadRequest": {"description": "bad", "content": {"application/json": {"schema": {"type": "object", "properties": {"message": {"type": "string"}}}}}}},
            },
        })
    }

    #[test]
    fn a_document_is_read_into_its_operations_in_path_and_method_order() {
        let spec = parse(&pets()).unwrap();
        assert_eq!(
            (spec.title.as_str(), spec.version.as_str()),
            ("Pet store", "1.2.0")
        );
        let ids: Vec<&str> = spec.operations.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, ["GET /pets", "POST /pets", "GET /pets/{petId}"]);
        assert!(spec.notes.is_empty(), "{:?}", spec.notes);
    }

    #[test]
    fn server_variables_are_replaced_by_their_defaults() {
        assert_eq!(
            parse(&pets()).unwrap().servers,
            ["https://petstore.example.test/api/v3"]
        );
    }

    #[test]
    fn an_operation_carries_its_names_summary_tags_and_deprecation() {
        let spec = parse(&pets()).unwrap();
        let list = &spec.operations[0];
        assert_eq!(list.operation_id.as_deref(), Some("listPets"));
        assert_eq!(list.summary.as_deref(), Some("List pets"));
        assert_eq!(list.tags, ["pets"]);
        assert!(!list.deprecated);
        assert!(spec.operations[2].deprecated);
        assert_eq!(
            (
                spec.operations[1].operation_id.clone(),
                spec.operations[1].summary.clone()
            ),
            (None, None)
        );
    }

    #[test]
    fn parameters_come_from_the_path_and_the_operation_with_references_resolved() {
        let spec = parse(&pets()).unwrap();
        let params: Vec<(String, Location, bool)> = spec.operations[0]
            .parameters
            .iter()
            .map(|p| (p.name.clone(), p.location, p.required))
            .collect();
        assert_eq!(
            params,
            [
                ("trace".to_string(), Location::Header, false),
                ("limit".to_string(), Location::Query, true),
                ("status".to_string(), Location::Query, false),
            ]
        );
        assert_eq!(
            spec.operations[0].parameters[2].schema,
            json!({"type": "string", "enum": ["a", "b"]})
        );
    }

    #[test]
    fn a_path_parameter_is_always_required() {
        let spec = parse(&pets()).unwrap();
        let pet_id = &spec.operations[2].parameters[0];
        assert_eq!(
            (pet_id.name.as_str(), pet_id.location, pet_id.required),
            ("petId", Location::Path, true)
        );
    }

    #[test]
    fn a_parameter_of_the_operation_replaces_one_of_the_path_with_the_same_name_and_place() {
        let mut doc = pets();
        doc["paths"]["/pets"]["get"]["parameters"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name": "trace", "in": "header", "required": true, "schema": {"type": "integer"}}));
        let spec = parse(&doc).unwrap();
        let trace: Vec<&Parameter> = spec.operations[0]
            .parameters
            .iter()
            .filter(|p| p.name == "trace")
            .collect();
        assert_eq!(trace.len(), 1);
        assert!(trace[0].required);
        assert_eq!(trace[0].schema, json!({"type": "integer"}));
    }

    #[test]
    fn the_request_body_is_the_json_one_when_there_is_one() {
        let spec = parse(&pets()).unwrap();
        let body = spec.operations[1].body.as_ref().unwrap();
        assert_eq!(body.content_type, "application/json");
        assert!(body.required);
        assert_eq!(body.schema["required"], json!(["name"]));
        assert!(spec.operations[0].body.is_none());
    }

    #[test]
    fn responses_are_listed_by_status_with_their_schemas_resolved() {
        let spec = parse(&pets()).unwrap();
        let statuses: Vec<&str> = spec.operations[0]
            .responses
            .iter()
            .map(|r| r.status.as_str())
            .collect();
        assert_eq!(statuses, ["200", "400"]);
        let ok = &spec.operations[0].responses[0];
        assert_eq!(ok.content_type.as_deref(), Some("application/json"));
        assert_eq!(ok.schema.as_ref().unwrap()["items"]["type"], "object");
        let bad = &spec.operations[0].responses[1];
        assert_eq!(
            bad.schema.as_ref().unwrap()["properties"]["message"]["type"],
            "string"
        );
        let created = &spec.operations[1].responses[0];
        assert_eq!(
            (created.content_type.clone(), created.schema.clone()),
            (None, None)
        );
    }

    #[test]
    fn a_schema_that_refers_to_itself_is_cut_short_not_followed_for_ever() {
        let spec = parse(&pets()).unwrap();
        let mut node = spec.operations[1].body.as_ref().unwrap().schema.clone();
        let mut depth = 0;
        while let Some(friend) = node
            .get("properties")
            .and_then(|p| p.get("friend"))
            .cloned()
        {
            node = friend;
            depth += 1;
            assert!(depth <= MAX_DEPTH, "never stops");
        }
        assert!(depth >= 2 && depth <= MAX_DEPTH, "followed {depth} levels");
        assert_eq!(node, json!({}), "the end of the chain is an empty schema");
    }

    #[test]
    fn what_wants_a_credential_is_the_operation_or_else_the_document() {
        let spec = parse(&pets()).unwrap();
        assert!(
            spec.operations[0].requires_auth,
            "the document's security applies"
        );
        assert!(!spec.operations[1].requires_auth, "security: [] says none");
        assert!(
            !spec.operations[2].requires_auth,
            "an empty requirement among the alternatives lets anyone in"
        );
    }

    #[test]
    fn a_document_with_no_security_wants_no_credential() {
        let mut doc = pets();
        doc.as_object_mut().unwrap().remove("security");
        assert!(!parse(&doc).unwrap().operations[0].requires_auth);
    }

    #[test]
    fn a_reference_outside_the_document_is_read_as_empty_and_said_so() {
        let mut doc = pets();
        doc["paths"]["/pets"]["get"]["responses"]["200"] = json!({"description": "x", "content": {"application/json": {"schema": {"$ref": "other.yaml#/Pet"}}}});
        let spec = parse(&doc).unwrap();
        assert_eq!(spec.operations[0].responses[0].schema, Some(json!({})));
        assert_eq!(spec.notes.len(), 1);
        assert!(
            spec.notes[0].contains("other.yaml#/Pet"),
            "{:?}",
            spec.notes
        );
    }

    #[test]
    fn a_reference_that_points_nowhere_is_read_as_empty_and_said_so_once() {
        let mut doc = pets();
        doc["components"]["schemas"]["Pet"]["properties"]["friend"] =
            json!({"$ref": "#/components/schemas/Gone"});
        doc["components"]["schemas"]["Pet"]["properties"]["enemy"] =
            json!({"$ref": "#/components/schemas/Gone"});
        let spec = parse(&doc).unwrap();
        assert_eq!(spec.notes.len(), 1);
        assert!(
            spec.notes[0].starts_with("1 reference(s)"),
            "{:?}",
            spec.notes
        );
    }

    #[test]
    fn a_json_pointer_is_unescaped() {
        let doc = json!({"a/b": {"c~d": 1}, "list": [10, 20]});
        assert_eq!(lookup(&doc, "#/a~1b/c~0d"), Some(&json!(1)));
        assert_eq!(lookup(&doc, "#/list/1"), Some(&json!(20)));
        assert_eq!(lookup(&doc, "#"), Some(&doc));
        assert_eq!(lookup(&doc, "#/list/x"), None);
        assert_eq!(lookup(&doc, "#/nope"), None);
        assert_eq!(lookup(&doc, "other.json#/a"), None);
        assert_eq!(lookup(&doc, "#/a~1b/c~0d/deeper"), None);
    }

    #[test]
    fn keys_beside_a_ref_are_kept_over_the_target() {
        let root = json!({"defs": {"A": {"type": "string", "description": "from the target", "minLength": 1}}});
        let mut unresolved = Vec::new();
        let resolved = resolve(
            &root,
            &json!({"$ref": "#/defs/A", "description": "written beside"}),
            &mut unresolved,
        );
        assert_eq!(
            resolved,
            json!({"type": "string", "description": "written beside", "minLength": 1})
        );
        assert!(unresolved.is_empty());
    }

    #[test]
    fn swagger_2_is_read_into_the_same_shape() {
        let doc = json!({
            "swagger": "2.0",
            "info": {"title": "Old API", "version": "1"},
            "host": "api.example.test", "basePath": "/v1/", "schemes": ["http", "https"],
            "consumes": ["application/json"], "produces": ["application/json"],
            "definitions": {"Item": {"type": "object", "required": ["id"], "properties": {"id": {"type": "integer"}}}},
            "paths": {"/items/{id}": {"put": {
                "parameters": [
                    {"name": "id", "in": "path", "required": true, "type": "integer", "minimum": 1},
                    {"name": "verbose", "in": "query", "type": "boolean"},
                    {"name": "item", "in": "body", "required": true, "schema": {"$ref": "#/definitions/Item"}},
                ],
                "responses": {"200": {"description": "ok", "schema": {"$ref": "#/definitions/Item"}}},
            }}},
        });
        let spec = parse(&doc).unwrap();
        assert_eq!(spec.servers, ["https://api.example.test/v1"]);
        let op = &spec.operations[0];
        assert_eq!(op.id, "PUT /items/{id}");
        assert_eq!(
            op.parameters.len(),
            2,
            "the body parameter is the body, not a parameter"
        );
        assert_eq!(
            op.parameters[0].schema,
            json!({"type": "integer", "minimum": 1})
        );
        assert_eq!(op.parameters[1].schema, json!({"type": "boolean"}));
        let body = op.body.as_ref().unwrap();
        assert_eq!(
            (body.content_type.as_str(), body.required),
            ("application/json", true)
        );
        assert_eq!(body.schema["required"], json!(["id"]));
        assert_eq!(
            op.responses[0].content_type.as_deref(),
            Some("application/json")
        );
        assert_eq!(
            op.responses[0].schema.as_ref().unwrap()["properties"]["id"]["type"],
            "integer"
        );
    }

    #[test]
    fn swagger_servers_fall_back_to_a_base_path_and_to_https() {
        let mut doc = json!({"swagger": "2.0", "paths": {"/a": {"get": {"responses": {}}}}});
        assert!(parse(&doc).unwrap().servers.is_empty());
        doc["basePath"] = json!("/api");
        assert_eq!(parse(&doc).unwrap().servers, ["/api"]);
        doc["host"] = json!("h.test");
        assert_eq!(parse(&doc).unwrap().servers, ["https://h.test/api"]);
        doc["schemes"] = json!(["http"]);
        assert_eq!(parse(&doc).unwrap().servers, ["http://h.test/api"]);
    }

    #[test]
    fn a_document_that_is_not_openapi_is_refused_in_words() {
        assert!(parse(&json!({"paths": {"/a": {}}}))
            .unwrap_err()
            .contains("neither an `openapi` nor a `swagger`"));
        assert!(parse(&json!({"openapi": "3.0.0"}))
            .unwrap_err()
            .contains("no paths"));
        assert!(parse(&json!({"openapi": "3.0.0", "paths": {}}))
            .unwrap_err()
            .contains("no paths"));
        assert!(
            parse(&json!({"openapi": "3.0.0", "paths": {"/a": {"summary": "nothing on it"}}}))
                .unwrap_err()
                .contains("no operation")
        );
    }

    #[test]
    fn a_missing_title_and_version_are_said_not_made_up() {
        let doc = json!({"openapi": "3.0.0", "paths": {"/a": {"get": {"responses": {}}}}});
        let spec = parse(&doc).unwrap();
        assert_eq!(
            (spec.title.as_str(), spec.version.as_str()),
            ("Untitled API", "unknown")
        );
    }

    #[test]
    fn a_document_with_too_many_operations_is_read_as_far_as_the_limit_and_says_so() {
        let mut paths = Map::new();
        for n in 0..(MAX_OPERATIONS + 5) {
            paths.insert(format!("/p{n:04}"), json!({"get": {"responses": {}}}));
        }
        let spec = parse(&json!({"openapi": "3.0.0", "paths": Value::Object(paths)})).unwrap();
        assert_eq!(spec.operations.len(), MAX_OPERATIONS);
        assert!(
            spec.notes[0].contains(&MAX_OPERATIONS.to_string()),
            "{:?}",
            spec.notes
        );
    }

    #[test]
    fn a_parameter_in_a_place_this_does_not_know_is_left_out() {
        let doc = json!({"openapi": "3.0.0", "paths": {"/a": {"get": {"parameters": [{"name": "x", "in": "elsewhere"}, {"name": "c", "in": "cookie"}], "responses": {}}}}});
        let spec = parse(&doc).unwrap();
        assert_eq!(spec.operations[0].parameters.len(), 1);
        assert_eq!(spec.operations[0].parameters[0].location, Location::Cookie);
    }
}
