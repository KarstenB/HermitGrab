//! Integration test ported from test.sh using Commands enum

use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use hermitgrab::commands::{self, AddCommand, Commands, GetCommand};
use hermitgrab::config::{
    FallbackOperation, GlobalConfig, PatchType, RequireTag, find_hermit_files,
};
use pretty_assertions::assert_eq;
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
        .replace("privateTEMP_DIR", "TEMP_DIR");
    serde_json::from_str(&data).expect("Failed to parse JSON")
}

fn assert_json_eq(expected: &Path, actual: &Path, temp_dir: &str) {
    let exp = read_json(expected, temp_dir);
    let act = read_json(actual, temp_dir);
    assert_eq!(
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

fn assert_symlink_points_to(link: &Path, target: &Path) {
    assert!(link.exists(), "Symlink does not exist: {}", link.display());
    let link_target = fs::read_link(link).expect("Not a symlink");
    assert_eq!(
        link_target,
        target,
        "Symlink {} does not point to {}",
        link.display(),
        target.display()
    );
}

fn assert_file_equals<P: AsRef<Path>>(path: P, content: &str) {
    assert_file_exists(&path);
    let data = fs::read_to_string(path).expect("Failed to read file");
    assert_eq!(content, data);
}

fn read_global_config(temp_path: &Path, hermit_root: &Path) -> Arc<GlobalConfig> {
    GlobalConfig::from_paths(hermit_root, temp_path, &find_hermit_files(hermit_root)).unwrap()
}

#[tokio::test]
async fn smoke_test() {
    let temp = TempDir::new().unwrap();
    let temp_path = temp.path();
    let temp_str = temp_path.to_str().unwrap();
    let _env_lock = ENV_LOCK.lock();
    unsafe {
        std::env::set_var("HOME", temp_path);
    }
    let hermit_root = temp_path.join(".hermitgrab");
    let cargo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_results = cargo_root.join("test_results");

    // Step 1: Init repo
    commands::execute(
        Commands::Init {
            init_command: commands::InitCommand::Create,
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        None,
    )
    .await
    .unwrap();
    assert_file_exists(hermit_root.join(".git/HEAD"));

    // Step 2: Create testfile.txt
    let testfile_txt = temp_path.join("testfile.txt");
    fs::write(&testfile_txt, "Test file content").unwrap();

    // Step 3: Add config test1
    commands::execute(
        Commands::Add {
            add_command: AddCommand::Config {
                config_dir: PathBuf::from("test1"),
                required_tags: vec![RequireTag::from_str("test1").unwrap()],
                order: None,
            },
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        None,
    )
    .await
    .unwrap();
    assert_file_exists(hermit_root.join("test1/hermit.toml"));
    // Compare config
    let expected = test_results.join("add_config_test1.json");
    let actual = test_results.join("add_config_test1_actual.json");
    commands::execute(
        Commands::Get {
            get_command: GetCommand::Config,
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        Some(actual.clone()),
    )
    .await
    .unwrap();
    assert_json_eq(&expected, &actual, temp_str);

    // Step 4: Add link for testfile.txt
    commands::execute(
        Commands::Add {
            add_command: AddCommand::Link {
                source: testfile_txt.clone(),
                config_dir: Some(PathBuf::from("test1")),
                link_type: hermitgrab::LinkType::Soft,
                target: None,
                required_tags: vec![],
                fallback: FallbackOperation::Abort,
                order: None,
            },
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        None,
    )
    .await
    .unwrap();
    assert_file_exists(hermit_root.join("test1/testfile.txt"));
    let expected = test_results.join("add_testfile_link.json");
    let actual = test_results.join("add_testfile_link_actual.json");
    commands::execute(
        Commands::Get {
            get_command: GetCommand::Config,
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        Some(actual.clone()),
    )
    .await
    .unwrap();
    assert_json_eq(&expected, &actual, temp_str);

    // Step 5: Add anotherfile.txt
    let anotherfile_txt = temp_path.join("anotherfile.txt");
    fs::write(&anotherfile_txt, "Another test file content").unwrap();
    commands::execute(
        Commands::Add {
            add_command: AddCommand::Link {
                source: anotherfile_txt.clone(),
                config_dir: Some(PathBuf::from("test1")),
                link_type: hermitgrab::LinkType::Soft,
                target: None,
                required_tags: vec![RequireTag::from_str("~another").unwrap()],
                fallback: FallbackOperation::BackupOverwrite,
                order: None,
            },
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        None,
    )
    .await
    .unwrap();
    assert_file_exists(hermit_root.join("test1/anotherfile.txt"));
    let expected = test_results.join("add_anotherfile_link.json");
    let actual = test_results.join("add_anotherfile_link_actual.json");
    commands::execute(
        Commands::Get {
            get_command: GetCommand::Config,
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        Some(actual.clone()),
    )
    .await
    .unwrap();
    assert_json_eq(&expected, &actual, temp_str);

    // Step 6: Status --tag test1
    let expected = test_results.join("unlinked_status.json");
    let actual = test_results.join("unlinked_status_actual.json");
    commands::execute(
        Commands::Status {
            tags: vec!["test1".to_string()],
            profile: None,
            extensive: false,
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        Some(actual.clone()),
    )
    .await
    .unwrap();
    assert_json_eq(&expected, &actual, temp_str);

    // Step 7: Add profile
    commands::execute(
        Commands::Add {
            add_command: AddCommand::Profile {
                name: "testProfile".to_string(),
                tags: vec!["hello".parse().unwrap(), "test1".parse().unwrap()],
            },
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        None,
    )
    .await
    .unwrap();
    assert_file_exists(hermit_root.join("hermit.toml"));
    let expected = test_results.join("add_profile_test1.json");
    let actual = test_results.join("add_profile_test1_actual.json");
    commands::execute(
        Commands::Get {
            get_command: GetCommand::Config,
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        Some(actual.clone()),
    )
    .await
    .unwrap();
    assert_json_eq(&expected, &actual, temp_str);

    // Step 8: Get tags
    // (output check omitted, but could parse stdout if needed)
    commands::execute(
        Commands::Get {
            get_command: GetCommand::Tags,
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        None,
    )
    .await
    .unwrap();

    // Step 9: Get profiles
    commands::execute(
        Commands::Get {
            get_command: GetCommand::Profiles,
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        None,
    )
    .await
    .unwrap();

    // Step 10: Apply expecting failure
    let expected = test_results.join("failed_apply.json");
    let actual = test_results.join("failed_apply_actual.json");
    commands::execute(
        Commands::Apply {
            tags: vec![],
            profile: Some("testProfile".to_string()),
            fallback: None,
            force: false,
            parallel: false,
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        Some(actual.clone()),
    )
    .await
    .unwrap();
    assert_json_eq(&expected, &actual, temp_str);

    // Step 11: Apply with force and parallel
    let expected = test_results.join("forced_apply.json");
    let actual = test_results.join("forced_apply_actual.json");
    commands::execute(
        Commands::Apply {
            tags: vec![],
            profile: Some("testProfile".to_string()),
            fallback: None,
            force: true,
            parallel: true,
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        Some(actual.clone()),
    )
    .await
    .unwrap();
    assert_json_eq(&expected, &actual, temp_str);
    assert_file_exists(temp_path.join("anotherfile.txt.bak"));
    assert_file_exists(temp_path.join("testfile.txt.bak"));
    assert_symlink_points_to(
        &temp_path.join("anotherfile.txt"),
        &hermit_root.join("test1/anotherfile.txt"),
    );
    assert_symlink_points_to(
        &temp_path.join("testfile.txt"),
        &hermit_root.join("test1/testfile.txt"),
    );
    let expected = test_results.join("linked_status.json");
    let actual = test_results.join("linked_status_actual.json");
    commands::execute(
        Commands::Status {
            tags: vec![],
            profile: Some("testProfile".to_string()),
            extensive: false,
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        Some(actual.clone()),
    )
    .await
    .unwrap();
    assert_json_eq(&expected, &actual, temp_str);

    // Step 12: Add patch
    let patch_toml = temp_path.join("patch.toml");
    fs::write(&patch_toml, "[alias]\n\"fa\" = \"format --all\"").unwrap();
    let cargo_dir = temp_path.join(".cargo");
    fs::create_dir_all(&cargo_dir).unwrap();
    let cargo_config = cargo_dir.join("config.toml");
    fs::write(&cargo_config, "[alias]\n\"ntr\" = \"nextest run\"").unwrap();
    commands::execute(
        Commands::Add {
            add_command: AddCommand::Patch {
                source: patch_toml.clone(),
                config_dir: Some(PathBuf::from("cargo")),
                patch_type: PatchType::JsonMerge,
                target: Some(cargo_config.clone()),
                required_tags: vec![RequireTag::from_str("cargo").unwrap()],
                order: Some(10),
            },
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        None,
    )
    .await
    .unwrap();
    let expected = test_results.join("add_patch.json");
    let actual = test_results.join("add_patch_actual.json");
    commands::execute(
        Commands::Get {
            get_command: GetCommand::Config,
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        Some(actual.clone()),
    )
    .await
    .unwrap();
    assert_json_eq(&expected, &actual, temp_str);
    assert_file_exists(hermit_root.join("cargo/hermit.toml"));
    assert_file_exists(hermit_root.join("cargo/patch.toml"));

    // Step 13: Apply patch
    let expected = test_results.join("applied_patch.json");
    let actual = test_results.join("applied_patch_actual.json");
    commands::execute(
        Commands::Apply {
            tags: vec!["cargo".to_string()],
            profile: None,
            fallback: None,
            force: false,
            parallel: false,
        },
        read_global_config(temp_path, &hermit_root),
        true,
        false,
        false,
        Some(actual.clone()),
    )
    .await
    .unwrap();
    assert_json_eq(&expected, &actual, temp_str);
    let expected = test_results.join("patched_config.toml");
    let actual = cargo_config;
    let exp = fs::read_to_string(&expected).unwrap();
    let act = fs::read_to_string(&actual).unwrap();
    assert_eq!(exp, act, "Patched config.toml differs");
}

#[tokio::test]
async fn parallel_ordered() {
    ordered_test(true).await
}

#[tokio::test]
async fn sequential_ordered() {
    ordered_test(false).await
}
async fn ordered_test(parallel: bool) {
    let temp = TempDir::new().unwrap();
    let temp_path = temp.path();
    let temp_str = temp_path.to_str().unwrap();
    let _env_lock = ENV_LOCK.lock();
    unsafe {
        std::env::set_var("HOME", temp_path);
    }
    let hermit_root = temp_path.join(".hermitgrab");
    let cargo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_results = cargo_root.join("test_results");
    let config_under_test = cargo_root.join("tests/test_exec_order/hermit.toml");
    eprintln!("config under test: {config_under_test:?}");
    std::fs::create_dir_all(&hermit_root).unwrap();
    std::fs::copy(config_under_test, hermit_root.join("hermit.toml")).unwrap();
    let expected = test_results.join("exec_ordered.json");
    let actual = test_results.join("exec_ordered_actual.json");
    commands::execute(
        Commands::Apply {
            tags: vec!["ordered".to_string()],
            profile: None,
            fallback: None,
            force: false,
            parallel,
        },
        read_global_config(temp_path, &hermit_root),
        true,
        true,
        false,
        Some(actual.clone()),
    )
    .await
    .unwrap();
    assert_file_equals(temp_path.join("test_exec_order.log"), "0\n1\n2\n10\n");
    assert_json_eq(&expected, &actual, temp_str);
}
