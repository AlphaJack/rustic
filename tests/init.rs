//! Rustic Integration Test for the chunker parameters of new repositories

use std::path::Path;

use assert_cmd::Command;
use rustic_testing::TestResult;
use serde_json::Value;
use tempfile::tempdir;

fn rustic(profile: &Path, repo: &Path) -> Command {
    let mut runner = Command::new(env!("CARGO_BIN_EXE_rustic"));
    runner
        .arg("--use-profile")
        .arg(profile)
        .arg("--repository")
        .arg(repo)
        .args(["--password", "test", "--no-progress", "--no-cache"]);
    runner
}

fn repo_config(profile: &Path, repo: &Path) -> TestResult<Value> {
    let output = rustic(profile, repo).args(["cat", "config"]).output()?;
    assert!(output.status.success());
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
fn init_copies_chunker_params_from_profile() -> TestResult<()> {
    let temp_dir = tempdir()?;
    let empty = temp_dir.path().join("empty.toml");
    let source = temp_dir.path().join("source");
    let target = temp_dir.path().join("target");
    let source_profile = temp_dir.path().join("source.toml");
    std::fs::write(&empty, "")?;
    std::fs::write(
        &source_profile,
        format!(
            "[repository]\nrepository = {:?}\npassword = \"test\"\n",
            source.display().to_string()
        ),
    )?;

    rustic(&empty, &source)
        .args([
            "init",
            "--set-chunk-size",
            "2MiB",
            "--set-chunk-min-size",
            "1MiB",
        ])
        .assert()
        .success();
    rustic(&empty, &target)
        .args([
            "init",
            "--set-chunk-size",
            "4MiB",
            "--copy-chunker-params-from",
        ])
        .arg(&source_profile)
        .assert()
        .success();

    let (source, target) = (repo_config(&empty, &source)?, repo_config(&empty, &target)?);
    for key in ["chunker_polynomial", "chunk_size", "chunk_min_size"] {
        assert_eq!(source[key], target[key], "{key}");
    }
    assert_ne!(source["id"], target["id"]);
    Ok(())
}

#[test]
fn init_sets_chunker_polynomial_from_profile_for_hot_cold() -> TestResult<()> {
    let temp_dir = tempdir()?;
    let cold = temp_dir.path().join("cold");
    let hot = temp_dir.path().join("hot");
    let source = temp_dir.path().join("source");
    let profile = temp_dir.path().join("test.toml");
    std::fs::write(
        &profile,
        "[init]\nset-chunker-polynomial = \"3DA3358B4DC173\"\n",
    )?;
    std::fs::create_dir(&source)?;
    std::fs::write(source.join("file"), "content")?;

    rustic(&profile, &cold)
        .arg("--repo-hot")
        .arg(&hot)
        .arg("init")
        .assert()
        .success();
    rustic(&profile, &cold)
        .arg("--repo-hot")
        .arg(&hot)
        .arg("backup")
        .arg(&source)
        .assert()
        .success();

    assert_eq!(
        repo_config(&profile, &cold)?["chunker_polynomial"],
        "3da3358b4dc173"
    );
    Ok(())
}

#[test]
fn init_rejects_invalid_chunker_polynomial() -> TestResult<()> {
    let temp_dir = tempdir()?;
    let empty = temp_dir.path().join("empty.toml");
    let repo = temp_dir.path().join("repo");
    std::fs::write(&empty, "")?;

    rustic(&empty, &repo)
        .args(["init", "--set-chunker-polynomial", "xyz"])
        .assert()
        .failure();
    assert!(!repo.exists());
    Ok(())
}
