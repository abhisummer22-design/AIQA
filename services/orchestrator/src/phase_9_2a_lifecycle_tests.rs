//! Phase 9.2A — focused tests for the test-fixture lifecycle.
//!
//! | test | Step 5 item |
//! |---|---|
//! | `p92a_fixture_creates_and_uses_a_real_database` | **A** |
//! | `p92a_migrations_run_on_every_fixture` | **B** |
//! | `p92a_pool_is_closed_before_the_directory_is_removed` | **C** |
//! | `p92a_sidecars_do_not_prevent_cleanup` | **D** |
//! | `p92a_repeated_use_does_not_accumulate_directories` | **E** |
//! | `p92a_concurrent_fixtures_do_not_interfere` | **F** |
//! | `p92a_teardown_errors_are_not_suppressed` | **G** |
//!
//! # Why the counts are not global
//!
//! Counting `.tmp*` across the whole system temp directory is not a valid assertion under
//! `cargo test`: these tests run in parallel with each other, so any *other* concurrently
//! running fixture perturbs the total and the result depends on scheduling order. Each test
//! therefore measures only inside a private root that it owns:
//! `%TEMP%\aiqa_p92a_artifacts\<test_name>\`.
//!
//! That root is namespaced to this file, is created fresh by the owning test, and is the only
//! thing ever removed. No historical artifact and no directory belonging to another test is
//! touched. Every assertion below reads the filesystem directly, so a green exit code is never
//! the evidence — the directory count is.

use std::path::{Path, PathBuf};

use crate::store::Store;

/// The private measurement root owned by one test. Clears only its own previous leaf.
fn artifact_root(test_name: &str) -> PathBuf {
    let root = std::env::temp_dir()
        .join("aiqa_p92a_artifacts")
        .join(test_name);
    // Scoped strictly to this file's namespace; never a blanket `.tmp*` sweep.
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create isolated artifact root");
    root
}

/// Number of child directories inside `root` — the only thing these tests assert on.
fn dir_count(root: &Path) -> usize {
    std::fs::read_dir(root)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .count()
        })
        .unwrap_or(0)
}

fn files_in(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Builds a fixture exactly the way `integrity_tests::harness` does, inside `root`.
async fn fixture_in(root: &Path, name: &str) -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir_in(root).expect("tempdir");
    let store = Store::connect(&dir.path().join(name))
        .await
        .expect("connect");
    (store, dir)
}

/// **A.** The helper must produce a usable database, not merely a directory.
#[tokio::test]
async fn p92a_fixture_creates_and_uses_a_real_database() {
    let root = artifact_root("a_usable_database");
    let (store, dir) = fixture_in(&root, "p92a-a.sqlite3").await;

    let pid = uuid::Uuid::new_v4();
    store
        .insert_project(pid, "P92A A", None)
        .await
        .expect("insert_project");
    let projects = store.list_projects(None).await.expect("list_projects");
    assert!(
        projects.iter().any(|p| p.id == pid),
        "the fixture's database must accept a real write and read it back"
    );
    assert!(
        dir.path().exists(),
        "the fixture owns a live directory while in use"
    );

    store.pool.close().await;
}

/// **B.** Migrations must run on every fixture. Test A's write would already fail on an unmigrated
/// database, but assert the schema directly so a regression has an unambiguous cause.
#[tokio::test]
async fn p92a_migrations_run_on_every_fixture() {
    let root = artifact_root("b_migrations");
    for name in ["p92a-b1.sqlite3", "p92a-b2.sqlite3"] {
        let (store, _dir) = fixture_in(&root, name).await;
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN \
             ('projects','test_cases','execution_telemetry','runs','quality_nodes')",
        )
        .fetch_one(&store.pool)
        .await
        .expect("query sqlite_master");
        assert_eq!(
            count.0, 5,
            "fixture {name} must have all 5 core tables migrated"
        );
        store.pool.close().await;
    }
    assert_eq!(
        dir_count(&root),
        0,
        "both migrated fixtures must have been cleaned up"
    );
}

/// **C + D.** The pool must be closed *before* the directory is removed, and the SQLite file plus
/// its WAL/SHM sidecars must not be what blocks the removal.
///
/// The directory path is captured so its absence can be checked after teardown — the only direct
/// evidence that `TempDir` removal actually happened rather than failing silently.
#[tokio::test]
async fn p92a_pool_is_closed_before_the_directory_is_removed() {
    let root = artifact_root("c_pool_closed_before_removal");
    let (store, dir) = fixture_in(&root, "p92a-c.sqlite3").await;
    let path = dir.path().to_path_buf();

    // Force real WAL/SHM sidecars into existence, so this test is not accidentally easier than
    // the fixtures it stands in for.
    sqlx::query("CREATE TABLE IF NOT EXISTS p92a_probe(x INTEGER)")
        .execute(&store.pool)
        .await
        .expect("create probe table");
    sqlx::query("INSERT INTO p92a_probe VALUES (1)")
        .execute(&store.pool)
        .await
        .expect("insert probe row");

    let present = files_in(&path);
    assert!(
        present.iter().any(|f| f == "p92a-c.sqlite3"),
        "the database file exists while the fixture is live: {present:?}"
    );
    assert!(
        present
            .iter()
            .any(|f| f.ends_with("-wal") || f.ends_with("-shm")),
        "WAL/SHM sidecars exist while the fixture is live, so the teardown below is a real test: \
         {present:?}"
    );

    // Teardown, in the order the remediated fixtures now use.
    store.pool.close().await;
    drop(dir);

    assert!(
        !path.exists(),
        "Step C/D: after an awaited pool close the directory must actually be gone. If it is \
         still present, an open SQLite handle (not the fixture) is what blocked removal."
    );
}

/// **E.** Repeated create/teardown must not accumulate. This is the regression guard for the whole
/// defect this phase addresses.
#[tokio::test]
async fn p92a_repeated_use_does_not_accumulate_directories() {
    let root = artifact_root("e_repeated_use");
    assert_eq!(dir_count(&root), 0, "the measurement root starts empty");

    for i in 0..10 {
        let (store, dir) = fixture_in(&root, &format!("p92a-e{i}.sqlite3")).await;
        store.pool.close().await;
        drop(dir);
    }

    let after = dir_count(&root);
    assert_eq!(
        after, 0,
        "ten fixture cycles must leave zero new temporary directories; {after} remained"
    );
}

/// **F.** Concurrent fixtures must each clean up only their own directory, and must not delete one
/// another's. Tasks are spawned onto the test runtime and awaited together, so they genuinely
/// interleave at their `.await` points rather than running one after another.
#[tokio::test]
async fn p92a_concurrent_fixtures_do_not_interfere() {
    let root = artifact_root("f_concurrent");
    assert_eq!(dir_count(&root), 0, "the measurement root starts empty");

    let mut handles = Vec::new();
    for i in 0..8 {
        let root = root.clone();
        handles.push(tokio::spawn(async move {
            let (store, dir) = fixture_in(&root, &format!("p92a-f{i}.sqlite3")).await;
            let path = dir.path().to_path_buf();
            // Assert the fixture is live before releasing it, so a fixture that failed to
            // create its database cannot pass by never existing.
            assert!(
                path.join(format!("p92a-f{i}.sqlite3")).exists(),
                "concurrent fixture {i} must have created its database"
            );
            store.pool.close().await;
            drop(dir);
            path
        }));
    }

    let mut paths = Vec::new();
    for handle in handles {
        paths.push(
            handle
                .await
                .expect("concurrent fixture task must not panic"),
        );
    }

    assert_eq!(
        dir_count(&root),
        0,
        "eight concurrent fixtures must leave nothing behind — neither their own directories nor \
         one another's"
    );
    for p in &paths {
        assert!(
            !p.exists(),
            "each concurrent fixture must remove exactly its own directory; {} survived",
            p.display()
        );
    }
}

/// **G.** A teardown failure must be visible rather than swallowed. `TempDir::close()` is the
/// fallible form and returns the real removal error; asserting on it is what stops a future
/// regression from being silent.
#[tokio::test]
async fn p92a_teardown_errors_are_not_suppressed() {
    let root = artifact_root("g_teardown_errors");
    let (store, dir) = fixture_in(&root, "p92a-g.sqlite3").await;
    store.pool.close().await;

    let path = dir.path().to_path_buf();
    let result = dir.close();
    assert!(
        result.is_ok(),
        "an awaited pool close must let TempDir::close() succeed; got {:?}",
        result.err()
    );
    assert!(
        !path.exists(),
        "and the directory must really be gone, not merely reported as closed"
    );
}
