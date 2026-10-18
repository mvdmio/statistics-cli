mod support;

use support::statistics;

#[test]
fn version_prints_the_crate_version_as_plain_text() {
    let run = statistics(&["--version"], &[]);

    assert_eq!(run.code, 0);
    assert_eq!(
        run.stdout,
        format!("statistics {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn no_command_is_a_usage_mistake() {
    let run = statistics(&[], &[]);

    assert_eq!(run.code, 2);
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(run.stdout.is_empty());
}
