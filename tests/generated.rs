mod support;

use std::fs::{self, File};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};
use support::{
    DESCRIPTION_PATH, FakeServer, Recorded, Reply, Run, TOKEN, body_json, fixture, statistics_in,
};
use tempfile::TempDir;

/// A fake Statistics host that serves an API description which a test can change between runs, and one cache
/// folder shared by every run of the test.
struct Host {
    server: FakeServer,
    description: Arc<Mutex<Option<Value>>>,
    cache: TempDir,
}

impl Host {
    fn start() -> Self {
        let description = Arc::new(Mutex::new(Some(fixture())));
        let served = Arc::clone(&description);
        let server = FakeServer::start(move |request| {
            if request.path() == DESCRIPTION_PATH {
                return match served.lock().unwrap().as_ref() {
                    Some(document) => Reply::json(200, &document.to_string()),
                    None => Reply::empty(503),
                };
            }
            Reply::json(200, "{\"done\":true}")
        });
        Host {
            server,
            description,
            cache: tempfile::tempdir().expect("a cache folder"),
        }
    }

    fn serve(&self, document: Option<Value>) {
        *self.description.lock().unwrap() = document;
    }

    fn run(&self, args: &[&str]) -> Run {
        self.run_in(&std::env::temp_dir(), args)
    }

    fn run_in(&self, dir: &std::path::Path, args: &[&str]) -> Run {
        let url = self.server.url();
        let cache = self.cache.path().to_str().expect("a UTF-8 path");
        statistics_in(
            dir,
            args,
            &[
                ("STATISTICS_URL", &url),
                ("STATISTICS_TOKEN", TOKEN),
                ("STATISTICS_CACHE_DIR", cache),
            ],
        )
    }

    fn description_fetches(&self) -> usize {
        self.server
            .requests()
            .iter()
            .filter(|request| request.path() == DESCRIPTION_PATH)
            .count()
    }

    fn api_requests(&self) -> Vec<Recorded> {
        self.server
            .requests()
            .into_iter()
            .filter(|request| request.path() != DESCRIPTION_PATH)
            .collect()
    }

    fn only_api_request(&self) -> Recorded {
        let requests = self.api_requests();
        assert_eq!(requests.len(), 1, "requests: {requests:#?}");
        requests.into_iter().next().unwrap()
    }

    /// Makes every cached copy look `age` old.
    fn age_cache(&self, age: Duration) {
        for entry in fs::read_dir(self.cache.path()).expect("the cache folder") {
            let file = File::options()
                .write(true)
                .open(entry.expect("a cache entry").path())
                .expect("open the cached copy");
            file.set_modified(SystemTime::now() - age)
                .expect("age the cached copy");
        }
    }
}

/// The fixture with one more operation, a POST to `path` whose `{name}` segments are string path parameters.
fn with_operation(group_action: &str, path: &str) -> Value {
    let parameters: Vec<Value> = path
        .split('/')
        .filter_map(|segment| segment.strip_prefix('{')?.strip_suffix('}'))
        .map(|name| json!({ "name": name, "in": "path", "required": true, "schema": { "type": "string" } }))
        .collect();
    let mut document = fixture();
    document["paths"][path] = json!({
        "post": {
            "tags": ["Projects"],
            "summary": "A new operation",
            "operationId": group_action,
            "parameters": parameters,
            "responses": { "200": { "description": "OK", "content": { "application/json": {} } } }
        }
    });
    document
}

/// The fixture with an operation whose body has a field of every type the CLI types.
fn with_typed_body() -> Value {
    let mut document = fixture();
    document["paths"]["/api/v1/projects/import"] = json!({
        "post": {
            "tags": ["Projects"],
            "summary": "Import a Project",
            "operationId": "projects.import",
            "requestBody": {
                "required": true,
                "content": { "application/json": { "schema": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string" },
                        "requestCount": { "type": "integer" },
                        "score": { "type": "number" },
                        "archived": { "type": "boolean" },
                        "memberIds": { "type": "array", "items": { "type": "string" } },
                        "startedOn": { "type": "string", "format": "date" },
                        "owner": { "type": "object" }
                    }
                } } }
            },
            "responses": { "200": { "description": "OK", "content": { "application/json": {} } } }
        }
    });
    document
}

/// The fixture with `projects.rename` taking a query parameter and a body field of the same name, and a body
/// field called `out`.
fn with_clashing_names() -> Value {
    let mut document = fixture();
    let rename = &mut document["paths"]["/api/v1/projects/{projectId}"]["patch"];
    rename["parameters"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "name": "name", "in": "query", "schema": { "type": "string" } }));
    rename["requestBody"]["content"]["application/json"]["schema"] = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "out": { "type": "string" }
        }
    });
    document
}

/// The fixture with the body of `projects.rename` left optional.
fn with_optional_body() -> Value {
    let mut document = fixture();
    document["paths"]["/api/v1/projects/{projectId}"]["patch"]["requestBody"]["required"] =
        json!(false);
    document
}

/// The names under `Commands:` in a `--help` answer.
fn listed_commands(help: &str) -> Vec<String> {
    help.lines()
        .skip_while(|line| line.trim_end() != "Commands:")
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

#[test]
fn an_action_with_a_path_parameter_sends_its_method_and_path() {
    let host = Host::start();

    let run = host.run(&["projects", "get", "7"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json(), json!({ "done": true }));
    let request = host.only_api_request();
    assert_eq!(request.method, "GET");
    assert_eq!(request.url, "/api/v1/projects/7");
    assert_eq!(
        request.header("Authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    assert!(request.body.is_empty());
}

#[test]
fn projects_list_prints_the_projects_as_json() {
    let projects = "{\"items\":[{\"projectId\":1,\"name\":\"Shop\"}]}";
    let server = FakeServer::start(move |request| {
        if request.path() == DESCRIPTION_PATH {
            return Reply::json(200, &fixture().to_string());
        }
        Reply::json(200, projects)
    });
    let cache = tempfile::tempdir().unwrap();
    let env = [
        ("STATISTICS_URL", server.url()),
        ("STATISTICS_TOKEN", TOKEN.to_string()),
        (
            "STATISTICS_CACHE_DIR",
            cache.path().to_str().unwrap().to_string(),
        ),
    ];
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();

    let generated = statistics_in(&std::env::temp_dir(), &["projects", "list"], &env);
    let raw = statistics_in(
        &std::env::temp_dir(),
        &["api", "GET", "/api/v1/projects"],
        &env,
    );

    assert_eq!(generated.code, 0, "{generated:#?}");
    assert_eq!(raw.code, 0, "{raw:#?}");
    let expected = json!({ "items": [{ "projectId": 1, "name": "Shop" }] });
    assert_eq!(generated.stdout_json(), expected);
    assert_eq!(raw.stdout_json(), expected);
    let api_urls: Vec<String> = server
        .requests()
        .into_iter()
        .filter(|request| request.path() != DESCRIPTION_PATH)
        .map(|request| request.url)
        .collect();
    assert_eq!(api_urls, ["/api/v1/projects", "/api/v1/projects"]);
}

#[test]
fn delete_sends_the_delete_method() {
    let host = Host::start();

    let run = host.run(&["projects", "delete", "7"]);

    assert_eq!(run.code, 0, "{run:#?}");
    let request = host.only_api_request();
    assert_eq!(request.method, "DELETE");
    assert_eq!(request.url, "/api/v1/projects/7");
}

#[test]
fn path_parameters_follow_the_path_order_and_are_encoded() {
    let host = Host::start();
    host.serve(Some(with_operation(
        "projects.pin-member",
        "/api/v1/projects/{projectId}/members/{memberId}/pin",
    )));

    let run = host.run(&["projects", "pin-member", "shop 1", "a/b"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        host.only_api_request().url,
        "/api/v1/projects/shop%201/members/a%2Fb/pin"
    );
}

#[test]
fn query_options_are_sent_and_a_repeatable_one_once_per_value() {
    let host = Host::start();

    let run = host.run(&[
        "requests",
        "list",
        "7",
        "--from",
        "2026-01-01",
        "--to",
        "2026-01-31",
        "--tag",
        "env=prod",
        "--tag",
        "env=dev",
        "--search",
        "flood",
        "--sort-by",
        "url",
        "--sort-direction",
        "desc",
        "--page",
        "2",
        "--page-size",
        "50",
    ]);

    assert_eq!(run.code, 0, "{run:#?}");
    let request = host.only_api_request();
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.url,
        "/api/v1/projects/7/requests?from=2026-01-01&to=2026-01-31&tag=env%3Dprod&tag=env%3Ddev\
         &search=flood&sortBy=url&sortDirection=desc&page=2&pageSize=50"
    );
}

#[test]
fn page_and_page_size_are_plain_integers_and_text_is_refused_before_any_request() {
    let host = Host::start();

    let run = host.run(&[
        "requests",
        "list",
        "7",
        "--from",
        "2026-01-01",
        "--to",
        "2026-01-31",
        "--page",
        "five",
    ]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn a_required_query_option_left_out_is_a_usage_mistake() {
    let host = Host::start();

    let run = host.run(&["requests", "list", "7", "--from", "2026-01-01"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn an_integer_path_parameter_refuses_text_before_any_request() {
    let host = Host::start();

    let run = host.run(&["projects", "get", "shop"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn a_body_field_option_is_sent_as_the_json_body() {
    let host = Host::start();

    let run = host.run(&["projects", "rename", "7", "--name", "Shop"]);

    assert_eq!(run.code, 0, "{run:#?}");
    let request = host.only_api_request();
    assert_eq!(request.method, "PATCH");
    assert_eq!(request.url, "/api/v1/projects/7");
    assert_eq!(request.header("Content-Type"), Some("application/json"));
    assert_eq!(body_json(&request), json!({ "name": "Shop" }));
}

#[test]
fn body_options_are_typed_from_the_schema() {
    let host = Host::start();
    host.serve(Some(with_typed_body()));

    let run = host.run(&[
        "projects",
        "import",
        "--name",
        "Shop",
        "--request-count",
        "3",
        "--score",
        "-2.5",
        "--archived",
        "true",
        "--member-ids",
        "a1",
        "--member-ids",
        "b2",
        "--started-on",
        "2026-01-02",
        "--owner",
        "{\"kind\":\"Person\",\"id\":\"p1\"}",
    ]);

    assert_eq!(run.code, 0, "{run:#?}");
    let request = host.only_api_request();
    assert_eq!(request.method, "POST");
    assert_eq!(request.header("Content-Type"), Some("application/json"));
    assert_eq!(
        body_json(&request),
        json!({
            "name": "Shop",
            "requestCount": 3,
            "score": -2.5,
            "archived": true,
            "memberIds": ["a1", "b2"],
            "startedOn": "2026-01-02",
            "owner": { "kind": "Person", "id": "p1" },
        })
    );
}

#[test]
fn an_integer_body_option_refuses_text_before_any_request() {
    let host = Host::start();
    host.serve(Some(with_typed_body()));

    let run = host.run(&["projects", "import", "--request-count", "five"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn field_options_win_over_the_same_keys_in_body() {
    let host = Host::start();

    let run = host.run(&[
        "projects",
        "rename",
        "7",
        "--body",
        "{\"name\":\"Old\",\"extra\":1}",
        "--name",
        "New",
    ]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        body_json(&host.only_api_request()),
        json!({ "name": "New", "extra": 1 })
    );
}

#[test]
fn body_reads_a_file() {
    let host = Host::start();
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("project.json"),
        "{\"name\":\"From a file\"}",
    )
    .unwrap();

    let run = host.run_in(
        dir.path(),
        &["projects", "rename", "7", "--body", "@project.json"],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        body_json(&host.only_api_request()),
        json!({ "name": "From a file" })
    );
}

#[test]
fn a_required_body_with_no_input_is_an_empty_object_and_an_optional_one_is_left_out() {
    let host = Host::start();

    let required = host.run(&["projects", "create"]);
    host.serve(Some(with_optional_body()));
    host.age_cache(Duration::from_secs(2 * 60 * 60));
    let optional = host.run(&["projects", "rename", "7"]);

    assert_eq!(required.code, 0, "{required:#?}");
    assert_eq!(optional.code, 0, "{optional:#?}");
    let requests = host.api_requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].body, b"{}");
    assert_eq!(requests[1].method, "PATCH");
    assert!(requests[1].body.is_empty());
    assert_eq!(requests[1].header("Content-Type"), None);
}

#[test]
fn a_clash_renames_the_later_option_and_help_says_so() {
    let host = Host::start();
    host.serve(Some(with_clashing_names()));

    let run = host.run(&[
        "projects",
        "rename",
        "7",
        "--name",
        "Query",
        "--field-name",
        "Body",
        "--out",
        "Transfer",
    ]);
    let help = host.run(&["projects", "rename", "--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    let request = host.api_requests().remove(0);
    assert_eq!(request.path(), "/api/v1/projects/7");
    assert_eq!(request.form("name").as_deref(), Some("Query"));
    assert_eq!(
        body_json(&request),
        json!({ "name": "Body", "out": "Transfer" })
    );
    assert!(
        help.stdout
            .contains("named --field-name because --name is taken"),
        "{}",
        help.stdout
    );
}

#[test]
fn help_on_an_action_shows_its_summary_description_and_parameters() {
    let host = Host::start();

    let get = host.run(&["projects", "get", "--help"]);
    let rename = host.run(&["projects", "rename", "--help"]);
    let list = host.run(&["requests", "list", "--help"]);

    assert_eq!(get.code, 0, "{get:#?}");
    for expected in [
        "Get a Project",
        "Returns the Project with its id and name.",
        "<projectId>",
        "Type: integer.",
    ] {
        assert!(get.stdout.contains(expected), "{expected}: {}", get.stdout);
    }
    for expected in [
        "Rename a Project",
        "A missing or blank `name` answers 422 on that field.",
        "--name <STRING>",
        "--body <JSON|@FILE>",
    ] {
        assert!(
            rename.stdout.contains(expected),
            "{expected}: {}",
            rename.stdout
        );
    }
    for expected in [
        "List Requests",
        "--from <STRING>",
        "--to <STRING>",
        "--tag <STRING>",
        "Repeat the option for more values.",
        "--sort-by <STRING>",
        "--sort-direction <STRING>",
        "--page <INTEGER>",
        "--page-size <INTEGER>",
    ] {
        assert!(
            list.stdout.contains(expected),
            "{expected}: {}",
            list.stdout
        );
    }
    assert!(host.api_requests().is_empty());
}

#[test]
fn help_on_a_group_lists_its_actions_and_its_tags() {
    let host = Host::start();

    let run = host.run(&["projects", "--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    for expected in [
        "list",
        "get",
        "create",
        "rename",
        "delete",
        "Rename a Project",
        "Tags: Projects.",
    ] {
        assert!(run.stdout.contains(expected), "{expected}: {}", run.stdout);
    }
}

#[test]
fn help_lists_the_hand_written_commands_and_the_generated_groups() {
    let host = Host::start();

    let run = host.run(&["--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    let commands = listed_commands(&run.stdout);
    for expected in [
        "login", "logout", "status", "accounts", "api", "skill", "projects", "requests",
    ] {
        assert!(
            commands.iter().any(|command| command == expected),
            "{expected}: {}",
            run.stdout
        );
    }
}

#[test]
fn help_lists_no_chat_command() {
    let host = Host::start();

    let help = host.run(&["--help"]);
    let chat = host.run(&["chat", "send", "hello"]);

    assert_eq!(help.code, 0, "{help:#?}");
    assert!(
        !listed_commands(&help.stdout).iter().any(|c| c == "chat"),
        "{}",
        help.stdout
    );
    assert!(
        !help.stdout.to_lowercase().contains("chat"),
        "{}",
        help.stdout
    );
    assert_eq!(chat.code, 2, "{chat:#?}");
    assert_eq!(chat.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn no_generated_action_offers_file_or_out() {
    let host = Host::start();
    let top = host.run(&["--help"]);
    let mut checked = 0;

    for group in listed_commands(&top.stdout) {
        if group == "help" {
            continue;
        }
        let group_help = host.run(&[&group, "--help"]);
        assert_eq!(group_help.code, 0, "{group}: {group_help:#?}");
        for action in listed_commands(&group_help.stdout) {
            if action == "help" {
                continue;
            }
            let help = host.run(&[&group, &action, "--help"]);
            assert_eq!(help.code, 0, "{group} {action}: {help:#?}");
            for option in ["--file", "--out"] {
                assert!(
                    !help.stdout.contains(option),
                    "{group} {action} offers {option}: {}",
                    help.stdout
                );
            }
            checked += 1;
        }
    }

    // The fixture's five `projects` actions and its one `requests` action, and the two hand-written `accounts`
    // actions that hide its two.
    assert_eq!(checked, 8, "{checked} actions were walked");
    assert!(host.api_requests().is_empty());
}

#[test]
fn a_second_run_within_the_hour_uses_the_cached_copy() {
    let host = Host::start();

    host.run(&["projects", "get", "1"]);
    let second = host.run(&["projects", "get", "2"]);

    assert_eq!(second.code, 0, "{second:#?}");
    assert_eq!(host.description_fetches(), 1);
}

#[test]
fn a_copy_older_than_an_hour_is_fetched_again() {
    let host = Host::start();
    host.run(&["projects", "get", "1"]);
    host.age_cache(Duration::from_secs(61 * 60));

    let run = host.run(&["projects", "get", "2"]);
    let after = host.run(&["projects", "get", "3"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(after.code, 0, "{after:#?}");
    assert_eq!(host.description_fetches(), 2);
}

#[test]
fn a_stale_copy_is_used_when_the_fetch_fails() {
    let host = Host::start();
    host.run(&["projects", "get", "1"]);
    host.age_cache(Duration::from_secs(2 * 60 * 60));
    host.serve(None);

    let run = host.run(&["projects", "get", "2"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.api_requests()[1].url, "/api/v1/projects/2");
}

#[test]
fn no_copy_and_no_fetch_is_a_network_error() {
    let host = Host::start();
    host.serve(None);

    let run = host.run(&["projects", "get", "1"]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "network");
    assert!(host.api_requests().is_empty());
}

#[test]
fn hand_written_commands_never_fetch_the_description() {
    let host = Host::start();
    host.serve(None);

    let run = host.run(&["api", "GET", "/api/v1/projects"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.description_fetches(), 0);
}

#[test]
fn an_operation_added_after_caching_works_through_the_refetch() {
    let host = Host::start();
    host.run(&["projects", "get", "1"]);
    host.serve(Some(with_operation(
        "projects.archive",
        "/api/v1/projects/{projectId}/archive",
    )));

    let run = host.run(&["projects", "archive", "1"]);
    let again = host.run(&["projects", "archive", "2"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(again.code, 0, "{again:#?}");
    assert_eq!(host.api_requests()[1].url, "/api/v1/projects/1/archive");
    assert_eq!(host.description_fetches(), 2);
}

#[test]
fn a_new_group_works_through_the_refetch() {
    let host = Host::start();
    host.run(&["projects", "get", "1"]);
    host.serve(Some(with_operation(
        "tags.clear",
        "/api/v1/projects/{projectId}/tags/clear",
    )));

    let run = host.run(&["tags", "clear", "1"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.api_requests()[1].url, "/api/v1/projects/1/tags/clear");
}

#[test]
fn a_command_still_unknown_after_the_refetch_is_a_usage_mistake() {
    let host = Host::start();
    host.run(&["projects", "get", "1"]);

    let action = host.run(&["projects", "explode", "1"]);
    let group = host.run(&["dragons", "list"]);

    assert_eq!(action.code, 2, "{action:#?}");
    assert_eq!(action.stderr_json()["error"], "usage");
    assert_eq!(group.code, 2, "{group:#?}");
    assert_eq!(host.description_fetches(), 3);
    assert_eq!(host.api_requests().len(), 1);
}

#[test]
fn the_hand_written_accounts_commands_win_over_the_generated_ones() {
    let host = Host::start();

    // The generated accounts.switch names its argument <accountId>; the hand-written one names it <ID>.
    let list = host.run(&["accounts", "list"]);
    let help = host.run(&["accounts", "switch", "--help"]);

    assert_eq!(list.code, 0, "{list:#?}");
    assert_eq!(host.only_api_request().url, "/api/v1/accounts");
    assert_eq!(help.code, 0, "{help:#?}");
    assert!(help.stdout.contains("<ID>"), "{}", help.stdout);
    assert!(!help.stdout.contains("<accountId>"), "{}", help.stdout);
}

#[test]
fn a_generated_browser_handoff_problem_is_printed_as_an_ordinary_problem() {
    const PROBLEM: &str = "{\"type\":\"browser-handoff\",\"detail\":\"Confirm in the browser.\",\"handoffUrl\":\"https://example.test/go\"}";
    let server = FakeServer::start(|request| {
        if request.path() == DESCRIPTION_PATH {
            return Reply::json(200, &fixture().to_string());
        }
        Reply::problem(403, PROBLEM)
    });

    let run = statistics_in(
        &std::env::temp_dir(),
        &["projects", "delete", "1"],
        &[
            ("STATISTICS_URL", &server.url()),
            ("STATISTICS_TOKEN", TOKEN),
        ],
    );

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr.trim_end(), PROBLEM);
    assert!(run.stdout.is_empty());
}

#[test]
fn a_problem_from_a_generated_action_lands_on_stderr_unchanged() {
    const PROBLEM: &str = "{\"type\":\"not-found\",\"title\":\"Not found\",\"status\":404,\"detail\":\"No Project 9.\"}";
    let server = FakeServer::start(|request| {
        if request.path() == DESCRIPTION_PATH {
            return Reply::json(200, &fixture().to_string());
        }
        Reply::problem(404, PROBLEM)
    });

    let run = statistics_in(
        &std::env::temp_dir(),
        &["projects", "get", "9"],
        &[
            ("STATISTICS_URL", &server.url()),
            ("STATISTICS_TOKEN", TOKEN),
        ],
    );

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr.trim_end(), PROBLEM);
    assert!(run.stdout.is_empty());
}

#[test]
fn help_without_the_description_lists_the_hand_written_commands_and_says_what_is_missing() {
    let host = Host::start();
    host.serve(None);

    let run = host.run(&["--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert!(run.stdout.contains("login"), "{}", run.stdout);
    assert!(!run.stdout.contains("projects"), "{}", run.stdout);
    assert!(
        run.stdout.contains("openapi/v1.json are missing"),
        "{}",
        run.stdout
    );
    assert_ends_with_the_docs_address(&host, &run);
}

fn assert_ends_with_the_docs_address(host: &Host, run: &Run) {
    let expected = format!("Docs: {}/agents", host.server.url());
    assert!(run.stdout.trim_end().ends_with(&expected), "{}", run.stdout);
}

#[test]
fn help_ends_with_the_docs_address_on_the_current_host() {
    let host = Host::start();

    let run = host.run(&["--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_ends_with_the_docs_address(&host, &run);
}

#[test]
fn an_unknown_command_with_a_failed_fetch_does_not_fetch_twice() {
    let host = Host::start();
    host.run(&["projects", "get", "1"]);
    host.age_cache(Duration::from_secs(2 * 60 * 60));
    host.serve(None);

    let run = host.run(&["projects", "explode", "1"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(host.description_fetches(), 2);
}
