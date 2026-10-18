use std::io::{self, Read};

use serde_json::{Value, json};
use ureq::http::{Response, header};

use crate::failure::{ErrorCode, Failure};
use crate::output;

/// Turns an API answer into the CLI's output. A success prints its JSON body, or `{"ok":true,"status":…}` when
/// the body is empty. A non-JSON body is refused, so no bytes reach stdout.
pub fn print_response(response: Response<ureq::Body>) -> Result<(), Failure> {
    let status = response.status().as_u16();
    let content_type = header_text(&response, header::CONTENT_TYPE);
    let mut reader = response.into_body().into_reader();

    if status >= 400 {
        return Err(problem(
            status,
            content_type.as_deref(),
            read_all(&mut reader)?,
        ));
    }

    if !content_type.as_deref().is_some_and(is_json) {
        let mut first = [0u8; 1];
        if reader.read(&mut first).map_err(network)? == 0 {
            return print_empty_success(status);
        }
        return Err(Failure::local(
            ErrorCode::BinaryResponse,
            format!(
                "The response is {}, not JSON.",
                content_type.as_deref().unwrap_or("of no stated type")
            ),
        ));
    }

    let bytes = read_all(&mut reader)?;
    if bytes.is_empty() {
        return print_empty_success(status);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(invalid_json)?;
    output::print_json(&value);
    Ok(())
}

/// The JSON body of a success, or the failure `print_response` would report for an error.
pub fn read_json(response: Response<ureq::Body>) -> Result<Value, Failure> {
    let status = response.status().as_u16();
    let content_type = header_text(&response, header::CONTENT_TYPE);
    let bytes = read_all(&mut response.into_body().into_reader())?;
    if status >= 400 {
        return Err(problem(status, content_type.as_deref(), bytes));
    }
    serde_json::from_slice(&bytes).map_err(invalid_json)
}

fn invalid_json(error: serde_json::Error) -> Failure {
    Failure::invalid_response(format!(
        "The response claims JSON but does not parse: {error}"
    ))
}

fn print_empty_success(status: u16) -> Result<(), Failure> {
    output::print_json(&json!({ "ok": true, "status": status }));
    Ok(())
}

/// The API's error body passes through as it came; one that is not JSON becomes a local `http` error.
fn problem(status: u16, content_type: Option<&str>, body: Vec<u8>) -> Failure {
    let is_api_problem = content_type
        .filter(|content_type| is_json(content_type))
        .is_some_and(|_| serde_json::from_slice::<Value>(&body).is_ok());
    if !is_api_problem {
        // An empty or non-JSON body (such as a proxy's HTML page) is not the API's; stderr stays JSON.
        return Failure::local(
            ErrorCode::Http,
            format!(
                "The server answered {status} with {}.",
                if body.is_empty() {
                    "no body"
                } else {
                    "a body that is not JSON"
                }
            ),
        );
    }
    Failure::Problem(body)
}

fn header_text(response: &Response<ureq::Body>, name: header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

pub fn is_json(content_type: &str) -> bool {
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    essence == "application/json" || essence.ends_with("+json")
}

fn read_all(reader: &mut impl Read) -> Result<Vec<u8>, Failure> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).map_err(network)?;
    Ok(bytes)
}

fn network(error: io::Error) -> Failure {
    Failure::local(ErrorCode::Network, error.to_string())
}
