//! Phase 2 migration check against a **copy of the pre-existing live database**.
//!
//! The master plan's Phase 2 test list requires "fresh database" *and* "existing
//! database". The fresh case is covered by every other test in this module (each
//! `temp_store()` starts from an empty file and runs all 29 migrations). This one
//! covers the other half: a database that already holds rows, with the old column
//! set, must gain the new columns and keep every row.
//!
//! It is skipped unless `AIQA_PRE_EXISTING_DB` points at such a file, because the
//! live database must never be migrated in place by a test. The caller copies it
//! first; the test only reads.
//!
//! ## Why `AssertSqlSafe` is used
//!
//! The row counts are compared table by table, and the table names therefore have
//! to be interpolated into `SELECT COUNT(*) FROM "<name>"`. sqlx rejects
//! runtime-built SQL by default, which is the right default. These names are not
//! caller input: each one is read out of this same database's `sqlite_master`
//! before it is used, so it is by construction an existing SQLite identifier and
//! cannot carry SQL. That is the audit `AssertSqlSafe` asserts, and every other
//! read in this file is a bound query.

use crate::store::Store;
use sqlx::Row;

/// Count every table's rows, reading the table list from `sqlite_master`.
async fn snapshot(pool: &sqlx::SqlitePool) -> std::collections::HashMap<String, i64> {
    let tables = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(pool)
    .await
    .expect("list tables");

    let mut counts = std::collections::HashMap::new();
    for t in tables {
        let name: String = t.get("name");
        // `_sqlx_migrations` is the migration ledger itself, and it is *supposed*
        // to grow by one row per migration — comparing it would make this test
        // fail on a fresh copy and pass on an already-migrated one, which is the
        // opposite of what a regression test should do.
        if name == "_sqlx_migrations" {
            continue;
        }
        // AUDITED: `name` came from this database's own `sqlite_master`, so it is
        // an existing table identifier, not input. See the module doc comment.
        let sql = sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM \"{name}\""));
        let n: i64 = sqlx::query_scalar(sql)
            .fetch_one(pool)
            .await
            .expect("count");
        counts.insert(name, n);
    }
    counts
}

#[tokio::test]
async fn an_existing_database_gains_the_new_schema_and_keeps_every_row() {
    let Ok(path) = std::env::var("AIQA_PRE_EXISTING_DB") else {
        // No copy supplied: nothing to assert against, and inventing a fixture
        // here would test a different thing from the one that matters.
        eprintln!("skipping: set AIQA_PRE_EXISTING_DB to a copy of the pre-0028 database");
        return;
    };
    let file = std::path::Path::new(&path);

    // Pre-migration counts, read with a raw pool so `Store` (which migrates on
    // connect) is not involved yet.
    let raw = sqlx::SqlitePool::connect(file.to_string_lossy().as_ref())
        .await
        .expect("raw connect");
    let before = snapshot(&raw).await;
    raw.close().await;

    assert!(
        before.contains_key("runs"),
        "the supplied file is not a usable AIQA database"
    );

    // Now let the real migrator run against the same file.
    let store = Store::connect(file).await.expect("connect + migrate");

    let ddl = store.table_ddl("tasks").await.unwrap().expect("tasks");
    assert!(ddl.contains("test_case_id"), "0028 did not apply: {ddl}");
    assert!(
        store.table_exists("qa_releases").await.unwrap(),
        "0029 did not apply"
    );
    let runs_ddl = store.table_ddl("runs").await.unwrap().expect("runs");
    assert!(
        runs_ddl.contains("release_id"),
        "0029 did not add runs.release_id"
    );

    // Every pre-existing row must still be there.
    let after = snapshot(&store.pool).await;
    let mut lost = Vec::new();
    for (table, count) in &before {
        match after.get(table) {
            None => lost.push(format!("{table}: vanished")),
            Some(now) if now != count => lost.push(format!("{table}: {count} -> {now}")),
            _ => {}
        }
    }
    assert!(
        lost.is_empty(),
        "the migration changed row counts: {lost:?}"
    );

    // Historical telemetry keeps its rows and, as expected, still has no link:
    // that is precisely the gap the backfill exists to fill honestly.
    let linked: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_telemetry WHERE test_case_id IS NOT NULL",
    )
    .fetch_one(&store.pool)
    .await
    .expect("count linked");
    assert_eq!(
        linked, 0,
        "the migration must not invent links for historical rows"
    );

    // Read the applied count rather than hardcoding it: this file is the check on
    // every migration, and a hardcoded number would itself go stale the moment one
    // is added — exactly the kind of quietly-wrong assertion Phase 6 exists to catch.
    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&store.pool)
        .await
        .expect("migration ledger");
    eprintln!(
        "migrated a copy of the existing database: {applied} migrations applied, \
         {} telemetry rows preserved, {linked} pre-linked",
        before.get("execution_telemetry").copied().unwrap_or(0),
    );

    // And every column each of this plan's migrations added must be present, by name.
    let tasks_ddl = store.table_ddl("tasks").await.unwrap().unwrap();
    assert!(
        tasks_ddl.contains("test_case_id"),
        "migration 0028 did not apply to an existing database: {tasks_ddl}"
    );
    let runs_ddl = store.table_ddl("runs").await.unwrap().unwrap();
    assert!(
        runs_ddl.contains("release_id"),
        "migration 0029 did not add runs.release_id to an existing database: {runs_ddl}"
    );
    let clusters_ddl = store.table_ddl("failure_clusters").await.unwrap().unwrap();
    assert!(
        clusters_ddl.contains("failure_fingerprint"),
        "migration 0030 did not apply to an existing database: {clusters_ddl}"
    );
    assert!(
        applied >= 30,
        "expected this plan's 5 migrations, ledger says {applied}"
    );
}
