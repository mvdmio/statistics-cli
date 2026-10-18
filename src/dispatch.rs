//! Turns the arguments of a generated command into its request.

use clap::ArgMatches;
use percent_encoding::utf8_percent_encode;
use serde_json::{Map, Value};

use crate::failure::Failure;
use crate::generated::{self, BODY};
use crate::http::Client;
use crate::openapi::Operation;
use crate::request::{self, UNRESERVED, path_segment};
use crate::response;

/// Builds the request of a generated command from its arguments and sends it.
pub fn run(operation: &Operation, matches: &ArgMatches) -> Result<(), Failure> {
    let body = body(operation, matches)?.map(|value| value.to_string().into_bytes());
    let mut client = Client::signed_in()?;
    let answer = client.send(
        &operation.method,
        &target(operation, matches),
        body.as_deref(),
    )?;
    response::print_response(answer)
}

/// The path with its placeholders filled, and the query string.
fn target(operation: &Operation, matches: &ArgMatches) -> String {
    let mut path = operation.path.clone();
    for parameter in &operation.path_params {
        if let Some(value) = matches.get_one::<Value>(&generated::path_id(parameter)) {
            let segment = path_segment(&text(value));
            path = path.replace(&format!("{{{}}}", parameter.name), &segment);
        }
    }
    let mut query = Vec::new();
    for parameter in &operation.query_params {
        for value in matches
            .get_many::<Value>(&generated::query_id(parameter))
            .into_iter()
            .flatten()
        {
            query.push(format!(
                "{}={}",
                utf8_percent_encode(&parameter.name, UNRESERVED),
                utf8_percent_encode(&text(value), UNRESERVED)
            ));
        }
    }
    if !query.is_empty() {
        path.push('?');
        path.push_str(&query.join("&"));
    }
    path
}

/// `--body` with the field options set on top of it. A required body with no input is `{}`.
fn body(operation: &Operation, matches: &ArgMatches) -> Result<Option<Value>, Failure> {
    let Some(body) = &operation.body else {
        return Ok(None);
    };
    let whole = matches
        .get_one::<String>(BODY)
        .map(|text| request::body_value(text))
        .transpose()?;
    let mut fields = Map::new();
    for field in &body.fields {
        let id = generated::field_id(field);
        let value = if field.schema.array {
            matches
                .get_many::<Value>(&id)
                .map(|values| Value::Array(values.cloned().collect()))
        } else {
            matches.get_one::<Value>(&id).cloned()
        };
        if let Some(value) = value {
            fields.insert(field.name.clone(), value);
        }
    }

    let has_fields = !fields.is_empty();
    let value = match whole {
        None if !has_fields && !body.required => return Ok(None),
        None => Value::Object(fields),
        Some(Value::Object(mut whole)) => {
            whole.extend(fields);
            Value::Object(whole)
        }
        Some(_) if has_fields => {
            return Err(Failure::Usage(
                "--body must be a JSON object when field options are given too.".to_string(),
            ));
        }
        Some(whole) => whole,
    };
    Ok(Some(value))
}

/// A string as it is; any other value as JSON text.
fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}
