use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hermitgrab::commands::{self, Commands};
use hermitgrab::config::{GlobalConfig, find_hermit_files};
use tempfile::TempDir;
use tokio::sync::Mutex;

/// This lock will be used for preventing modification of the HOME variable
/// when multiple tests are started simultaenously. This also means that
/// only one test can run at a time. This is due to the usage of the $HOME
/// variable in the ordered test.
static ENV_LOCK: Mutex<()> = Mutex::const_new(());

fn read_json<P: AsRef<Path>>(path: P, temp_dir: &str) -> serde_json::Value {
    assert_file_exists(&path);
    let data = fs::read_to_string(path).expect("Failed to read file");
    let data = data
        .replace(temp_dir, "TEMP_DIR")
        .replace("/privateTEMP_DIR", "TEMP_DIR");
    serde_json::from_str(&data).expect("Failed to parse JSON")
}

fn assert_json_eq(expected: &Path, actual: &Path, temp_dir: &str) {
    let exp = read_json(expected, temp_dir);
    let act = read_json(actual, temp_dir);
    pretty_assertions::assert_eq!(
        exp,
        act,
        "JSON files differ: {} != {}",
        expected.display(),
        actual.display()
    );
}

fn assert_file_exists<P: AsRef<Path>>(path: P) {
    assert!(
        path.as_ref().exists(),
        "File does not exist: {}",
        path.as_ref().display()
    );
}

fn read_global_config(hermit_root: &Path) -> Arc<GlobalConfig> {
    GlobalConfig::from_paths(hermit_root, &find_hermit_files(hermit_root)).unwrap()
}

#[tokio::test]
async fn xdg_cache() {
    test_env_subst("XDG_CACHE_HOME", false).await
}
#[tokio::test]
async fn xdg_cache_clean() {
    test_env_subst("XDG_CACHE_HOME", true).await
}

#[tokio::test]
async fn xdg_config() {
    test_env_subst("XDG_CONFIG_HOME", false).await
}
#[tokio::test]
async fn xdg_config_clean() {
    test_env_subst("XDG_CONFIG_HOME", true).await
}

#[tokio::test]
async fn xdg_data() {
    test_env_subst("XDG_DATA_HOME", false).await
}
#[tokio::test]
async fn xdg_data_clean() {
    test_env_subst("XDG_DATA_HOME", true).await
}

#[tokio::test]
async fn xdg_executable() {
    test_env_subst("XDG_BIN_HOME", false).await
}
#[tokio::test]
async fn xdg_executable_clean() {
    test_env_subst("XDG_BIN_HOME", true).await
}

#[tokio::test]
async fn xdg_runtime() {
    test_env_subst("XDG_RUNTIME_DIR", false).await
}
#[tokio::test]
async fn xdg_runtime_clean() {
    test_env_subst("XDG_RUNTIME_DIR", true).await
}

#[tokio::test]
async fn xdg_state() {
    test_env_subst("XDG_STATE_HOME", false).await
}
#[tokio::test]
async fn xdg_state_clean() {
    test_env_subst("XDG_STATE_HOME", true).await
}

async fn test_env_subst(env_name: &str, clean: bool) {
    let temp = TempDir::new().unwrap();
    let temp_path = temp.path();
    let temp_str = temp_path.to_str().unwrap();
    let _env_lock = ENV_LOCK.lock().await;
    unsafe {
        std::env::set_var("HOME", temp_path);
        if clean {
            std::env::remove_var("XDG_HOME");
            std::env::remove_var("XDG_CACHE_HOME");
            std::env::remove_var("XDG_CONFIG_HOME");
            std::env::remove_var("XDG_DATA_HOME");
            std::env::remove_var("XDG_BIN_HOME");
            // std::env::remove_var("XDG_RUNTIME_DIR");
            std::env::set_var("XDG_RUNTIME_DIR", temp_path.join("xdg_runtime_dir_default"));
            std::env::remove_var("XDG_STATE_HOME");
        } else {
            std::env::set_var("XDG_HOME", temp_path);
            std::env::set_var("XDG_CACHE_HOME", temp_path.join("xdg_cache_dir"));
            std::env::set_var("XDG_CONFIG_HOME", temp_path.join("xdg_config_dir"));
            std::env::set_var("XDG_DATA_HOME", temp_path.join("xdg_data_dir"));
            std::env::set_var("XDG_BIN_HOME", temp_path.join("xdg_bin_dir"));
            std::env::set_var("XDG_RUNTIME_DIR", temp_path.join("xdg_runtime_dir"));
            std::env::set_var("XDG_STATE_HOME", temp_path.join("xdg_state_dir"));
        }
        std::env::set_var(env_name, temp_path.join(env_name));
    }
    let hermit_root = temp_path.join(".hermitgrab");
    let cargo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_results = cargo_root.join("test_results");
    let config_under_test = cargo_root.join("tests/test_xdg_variables/hermit.toml");
    eprintln!("config under test: {config_under_test:?}");
    std::fs::create_dir_all(&hermit_root).unwrap();
    std::fs::copy(config_under_test, hermit_root.join("hermit.toml")).unwrap();
    let fish_file = cargo_root.join("tests/test_xdg_variables/config.fish");
    std::fs::copy(fish_file, hermit_root.join("config.fish")).unwrap();
    let completions_dir = hermit_root.join("completions");
    std::fs::create_dir_all(&completions_dir).unwrap();
    let rustup_fish = cargo_root.join("tests/test_xdg_variables/completions/rustup.fish");
    std::fs::copy(rustup_fish, completions_dir.join("rustup.fish")).unwrap();
    let expected = test_results.join(format!("exec_env_{env_name}_{clean}_expected.json"));
    let actual = test_results.join(format!("exec_env_{env_name}_{clean}_actual.json"));
    commands::execute(
        Commands::Apply {
            tags: vec!["xdg_test".to_string()],
            profile: None,
            fallback: None,
            force: false,
            parallel: false,
        },
        read_global_config(&hermit_root),
        true,
        true,
        false,
        Some(actual.clone()),
    )
    .await
    .unwrap();
    assert_json_eq(&expected, &actual, temp_str);
    unsafe { std::env::remove_var(env_name) };
}
