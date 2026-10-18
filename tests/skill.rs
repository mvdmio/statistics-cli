mod support;

use std::fs;
use std::path::Path;

use support::{FakeServer, Reply, statistics};

#[test]
fn skill_prints_the_repo_skill_file_without_a_request() {
    let server = FakeServer::start(|_| Reply::empty(500));
    let repo_file = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("SKILL.md"))
        .expect("read SKILL.md");

    let run = statistics(&["skill"], &[("STATISTICS_URL", &server.url())]);

    assert_eq!(run.code, 0);
    assert_eq!(run.stdout, repo_file);
    assert!(run.stderr.is_empty());
    assert!(server.requests().is_empty());
}

#[test]
fn skill_starts_with_front_matter_a_skill_loader_accepts() {
    let run = statistics(&["skill"], &[]);

    let front_matter = run
        .stdout
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(front_matter, _)| front_matter)
        .expect("front matter between --- lines");
    assert!(front_matter.lines().any(|line| line == "name: statistics"));
    assert!(
        front_matter
            .lines()
            .any(|line| line.starts_with("description: ") && line.len() > "description: ".len())
    );
}

#[test]
fn skill_names_the_statistics_cli_its_token_and_its_install_lines() {
    let run = statistics(&["skill"], &[]);

    for expected in [
        "statistics",
        "STATISTICS_TOKEN",
        "STATISTICS_URL",
        "curl -LsSf https://statistics.mvdm.io/cli/install.sh | sh",
        "powershell -ExecutionPolicy ByPass -c \"irm https://statistics.mvdm.io/cli/install.ps1 | iex\"",
    ] {
        assert!(run.stdout.contains(expected), "skill lacks {expected}");
    }
}

#[test]
fn skill_mentions_no_cut_feature() {
    let run = statistics(&["skill"], &[]);
    let skill = run.stdout.to_lowercase();

    for absent in [
        "assistant",
        "handoff",
        "chat",
        "upload",
        "download",
        "odata",
        "--file",
        "--out",
    ] {
        assert!(!skill.contains(absent), "skill still mentions {absent}");
    }
    assert!(!run.stdout.contains("exit 3"));
    assert!(!run.stdout.contains("| 3 |"));
}

#[test]
fn skill_names_no_other_product_cli_or_token_variable() {
    let run = statistics(&["skill"], &[]);

    let variables: Vec<&str> = run
        .stdout
        .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
        .filter(|word| word.ends_with("_TOKEN") || word.ends_with("_URL"))
        .collect();
    assert!(
        variables
            .iter()
            .all(|word| matches!(*word, "STATISTICS_TOKEN" | "STATISTICS_URL")),
        "skill names {variables:?}"
    );
    for other in ["healthcheck", "translationtools", "commonplace"] {
        assert!(
            !run.stdout.to_lowercase().contains(other),
            "skill names {other}"
        );
    }
    let hosts: Vec<&str> = run
        .stdout
        .match_indices("https://")
        .map(|(start, _)| &run.stdout[start..])
        .map(|rest| rest.split('/').nth(2).unwrap_or_default())
        .collect();
    assert!(
        hosts
            .iter()
            .all(|host| host.starts_with("statistics.mvdm.io")),
        "skill names hosts {hosts:?}"
    );
}
