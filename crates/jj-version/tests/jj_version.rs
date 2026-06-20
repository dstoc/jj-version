use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn cargo_bin() -> PathBuf {
    tool_path("cargo")
}

fn jj_bin() -> PathBuf {
    tool_path("jj")
}

fn tool_path(tool: &str) -> PathBuf {
    let output = Command::new("sh")
        .args(["-c", &format!("command -v {tool}")])
        .output()
        .expect("tool lookup should run");
    assert!(
        output.status.success(),
        "failed to locate {tool}: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let path = String::from_utf8(output.stdout).expect("tool path should be utf8");
    PathBuf::from(path.trim())
}

fn path_with_prepended_dir(dir: &Path) -> String {
    let original_path = std::env::var("PATH").expect("PATH should be set");
    format!("{}:{original_path}", dir.display())
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directories");
    }
    fs::write(path, contents).expect("write file");
}

fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = fs::metadata(path)
            .expect("executable metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("set executable permissions");
    }
}

fn run_ok(command: &mut Command) -> Output {
    let output = command.output().expect("command should run");
    assert!(
        output.status.success(),
        "command failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_fail(command: &mut Command) -> Output {
    let output = command.output().expect("command should run");
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn init_repo(repo_root: &Path) {
    run_ok(
        Command::new(jj_bin())
            .arg("--quiet")
            .arg("git")
            .arg("init")
            .arg("--colocate")
            .arg(repo_root),
    );
}

fn snapshot_working_copy(repo_root: &Path) {
    run_ok(
        Command::new(jj_bin())
            .current_dir(repo_root)
            .arg("--quiet")
            .arg("status"),
    );
}

fn new_empty_change(repo_root: &Path) {
    run_ok(
        Command::new(jj_bin())
            .current_dir(repo_root)
            .arg("--quiet")
            .arg("new")
            .arg("--no-edit"),
    );
}

fn tag_revision(repo_root: &Path, tag: &str, revision: &str) {
    run_ok(
        Command::new(jj_bin())
            .current_dir(repo_root)
            .arg("--quiet")
            .arg("tag")
            .arg("set")
            .arg("--revision")
            .arg(revision)
            .arg(tag),
    );
}

fn effective_short_id(repo_root: &Path) -> String {
    let output = run_ok(Command::new(jj_bin()).current_dir(repo_root).args([
        "--ignore-working-copy",
        "--at-op=@",
        "--no-pager",
        "--color=never",
        "log",
        "-G",
        "-r",
        "coalesce(@ ~ empty(), @-)",
        "-T",
        r#"commit_id.short(12) ++ "\n""#,
    ]));
    let stdout = String::from_utf8(output.stdout).expect("jj output should be utf8");
    stdout.trim().to_owned()
}

fn create_downstream_crate(crate_dir: &Path, dep_path: &Path, fallback_expr: &str) {
    let cargo_toml = format!(
        r#"[package]
name = "downstream"
version = "0.1.0"
edition = "2024"

[dependencies]
jj-version = {{ path = "{}" }}
"#,
        dep_path.display()
    );

    let main_rs = format!(
        r#"fn main() {{
    println!("{{}}", jj_version::version!(fallback = {fallback_expr}));
}}
"#
    );
    write_file(&crate_dir.join("Cargo.toml"), &cargo_toml);
    write_file(&crate_dir.join("src/main.rs"), &main_rs);
}

fn cargo_run(crate_dir: &Path, target_dir: &Path, extra_env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(cargo_bin());
    command
        .current_dir(crate_dir)
        .arg("run")
        .arg("--quiet")
        .arg("--offline")
        .env("CARGO_TARGET_DIR", target_dir);

    for (key, value) in extra_env {
        command.env(key, value);
    }

    run_ok(&mut command)
}

fn cargo_check(crate_dir: &Path, target_dir: &Path, extra_env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(cargo_bin());
    command
        .current_dir(crate_dir)
        .arg("check")
        .arg("--quiet")
        .arg("--offline")
        .env("CARGO_TARGET_DIR", target_dir);

    for (key, value) in extra_env {
        command.env(key, value);
    }

    run_fail(&mut command)
}

fn repo_tempdir(prefix: &str) -> TempDir {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(std::env::temp_dir())
        .expect("create tempdir")
}

fn prepare_repo_state(repo_root: &Path, contents: &str) {
    write_file(&repo_root.join("state.txt"), contents);
    snapshot_working_copy(repo_root);
}

fn create_jj_wrapper_dir(repo_root: &Path) -> TempDir {
    let dir = repo_tempdir("jj-version-jj-path-");
    let wrapper = dir.path().join("jj");
    write_file(
        &wrapper,
        &format!(
            "#!/bin/sh\nexec {} --repository {} \"$@\"\n",
            jj_bin().display(),
            repo_root.display()
        ),
    );

    make_executable(&wrapper);

    dir
}

fn create_failing_jj_dir() -> TempDir {
    let dir = repo_tempdir("jj-version-missing-jj-path-");
    let wrapper = dir.path().join("jj");
    write_file(&wrapper, "#!/bin/sh\nexit 127\n");
    make_executable(&wrapper);
    dir
}

#[test]
fn falls_back_when_not_in_a_jj_repository() {
    let temp = repo_tempdir("jj-version-no-repo-");
    let dep_crate = crate_root();
    let crate_dir = temp.path().join("app");
    let target_dir = temp.path().join("target");

    create_downstream_crate(&crate_dir, &dep_crate, r#"env!("CARGO_PKG_VERSION")"#);
    let output = cargo_run(&crate_dir, &target_dir, &[]);
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    assert_eq!(stdout.trim(), "0.1.0");
}

#[test]
fn falls_back_when_jj_is_missing_from_path() {
    let temp = repo_tempdir("jj-version-path-fallback-");
    let dep_crate = crate_root();
    let repo_root = temp.path().join("repo");
    let crate_dir = repo_root.join("app");
    let target_dir = temp.path().join("target");

    fs::create_dir_all(&repo_root).expect("create repo root");
    init_repo(&repo_root);
    prepare_repo_state(&repo_root, "path fallback\n");
    fs::create_dir_all(&crate_dir).expect("create crate dir");

    create_downstream_crate(
        &crate_dir,
        &dep_crate,
        r#"concat!("fallback-", stringify!(nested_tokens), "-", env!("CARGO_PKG_VERSION"))"#,
    );

    let failing_jj = create_failing_jj_dir();
    let path_value = path_with_prepended_dir(failing_jj.path());
    let output = cargo_run(&crate_dir, &target_dir, &[("PATH", path_value.as_str())]);
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    assert_eq!(stdout.trim(), "fallback-nested_tokens-0.1.0");
}

#[test]
fn no_tags_outputs_short_commit_id() {
    let temp = repo_tempdir("jj-version-no-tags-");
    let dep_crate = crate_root();
    let repo_root = temp.path().join("repo");
    let crate_dir = repo_root.join("app");
    let target_dir = temp.path().join("target");

    fs::create_dir_all(&repo_root).expect("create repo root");
    init_repo(&repo_root);
    prepare_repo_state(&repo_root, "no tags\n");
    let expected = effective_short_id(&repo_root);
    fs::create_dir_all(&crate_dir).expect("create crate dir");

    create_downstream_crate(&crate_dir, &dep_crate, r#"env!("CARGO_PKG_VERSION")"#);
    let jj_path = create_jj_wrapper_dir(&repo_root);
    let path_value = path_with_prepended_dir(jj_path.path());
    let output = cargo_run(&crate_dir, &target_dir, &[("PATH", path_value.as_str())]);
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    assert_eq!(stdout.trim(), expected);
}

#[test]
fn exact_tag_outputs_tag() {
    let temp = repo_tempdir("jj-version-tagged-");
    let dep_crate = crate_root();
    let repo_root = temp.path().join("repo");
    let crate_dir = repo_root.join("app");
    let target_dir = temp.path().join("target");

    fs::create_dir_all(&repo_root).expect("create repo root");
    init_repo(&repo_root);
    prepare_repo_state(&repo_root, "tagged\n");
    tag_revision(&repo_root, "v1.2.3", "@");
    fs::create_dir_all(&crate_dir).expect("create crate dir");

    create_downstream_crate(&crate_dir, &dep_crate, r#"env!("CARGO_PKG_VERSION")"#);
    let jj_path = create_jj_wrapper_dir(&repo_root);
    let path_value = path_with_prepended_dir(jj_path.path());
    let output = cargo_run(&crate_dir, &target_dir, &[("PATH", path_value.as_str())]);
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    assert_eq!(stdout.trim(), "v1.2.3");
}

#[test]
fn ahead_of_tag_outputs_described_version() {
    let temp = repo_tempdir("jj-version-ahead-");
    let dep_crate = crate_root();
    let repo_root = temp.path().join("repo");
    let crate_dir = repo_root.join("app");
    let target_dir = temp.path().join("target");

    fs::create_dir_all(&repo_root).expect("create repo root");
    init_repo(&repo_root);
    prepare_repo_state(&repo_root, "base\n");
    tag_revision(&repo_root, "v2.0.0", "@");

    prepare_repo_state(&repo_root, "ahead\n");
    let short = effective_short_id(&repo_root);
    fs::create_dir_all(&crate_dir).expect("create crate dir");

    create_downstream_crate(&crate_dir, &dep_crate, r#"env!("CARGO_PKG_VERSION")"#);
    let jj_path = create_jj_wrapper_dir(&repo_root);
    let path_value = path_with_prepended_dir(jj_path.path());
    let output = cargo_run(&crate_dir, &target_dir, &[("PATH", path_value.as_str())]);
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    assert_eq!(stdout.trim(), format!("v2.0.0-1-g{short}"));
}

#[test]
fn empty_working_copy_uses_parent_commit() {
    let temp = repo_tempdir("jj-version-empty-wc-");
    let dep_crate = crate_root();
    let repo_root = temp.path().join("repo");
    let crate_dir = repo_root.join("app");
    let target_dir = temp.path().join("target");

    fs::create_dir_all(&repo_root).expect("create repo root");
    init_repo(&repo_root);
    prepare_repo_state(&repo_root, "empty\n");
    let parent = effective_short_id(&repo_root);
    new_empty_change(&repo_root);
    fs::create_dir_all(&crate_dir).expect("create crate dir");

    create_downstream_crate(&crate_dir, &dep_crate, r#"env!("CARGO_PKG_VERSION")"#);
    let jj_path = create_jj_wrapper_dir(&repo_root);
    let path_value = path_with_prepended_dir(jj_path.path());
    let output = cargo_run(&crate_dir, &target_dir, &[("PATH", path_value.as_str())]);
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    assert_eq!(stdout.trim(), parent);
}

#[test]
fn non_empty_working_copy_uses_current_commit_without_dirty_suffix() {
    let temp = repo_tempdir("jj-version-non-empty-wc-");
    let dep_crate = crate_root();
    let repo_root = temp.path().join("repo");
    let crate_dir = repo_root.join("app");
    let target_dir = temp.path().join("target");

    fs::create_dir_all(&repo_root).expect("create repo root");
    init_repo(&repo_root);
    prepare_repo_state(&repo_root, "first\n");
    prepare_repo_state(&repo_root, "second\n");
    let current = effective_short_id(&repo_root);
    fs::create_dir_all(&crate_dir).expect("create crate dir");

    create_downstream_crate(&crate_dir, &dep_crate, r#"env!("CARGO_PKG_VERSION")"#);
    let jj_path = create_jj_wrapper_dir(&repo_root);
    let path_value = path_with_prepended_dir(jj_path.path());
    let output = cargo_run(&crate_dir, &target_dir, &[("PATH", path_value.as_str())]);
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    assert_eq!(stdout.trim(), current);
    assert!(!stdout.contains("dirty"));
}

#[test]
fn syntax_errors_are_reported() {
    let temp = repo_tempdir("jj-version-syntax-");
    let dep_crate = crate_root();
    let crate_dir = temp.path().join("app");
    let target_dir = temp.path().join("target");

    create_downstream_crate(&crate_dir, &dep_crate, r#"env!("CARGO_PKG_VERSION")"#);
    write_file(
        &crate_dir.join("src/main.rs"),
        r#"fn main() {
    let _ = jj_version::version!(fallback = );
}
"#,
    );

    let output = cargo_check(&crate_dir, &target_dir, &[]);
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");

    assert!(
        stderr.contains("expected `fallback = <expr>`"),
        "unexpected stderr:\n{stderr}"
    );
}
