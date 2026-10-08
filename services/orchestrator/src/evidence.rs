//! Evidence storage — build report: "Evidence storage (S3/MinIO): Not
//! wired — no screenshots/traces are captured or uploaded anywhere."
//!
//! Backed by the local filesystem rather than MinIO/S3, for the same reason
//! `store.rs` uses SQLite instead of Postgres: there's no container runtime
//! in this dev environment to run MinIO in (`infra/docker-compose.yml`
//! defines it, but nothing here can start it). The contract SRD §15
//! actually cares about — evidence is an immutable, content-addressed-by-key
//! blob referenced from the DB by `object_key`, never inlined into a DB row
//! or handed to a model directly — holds here too; only the storage backend
//! differs. Swapping to an S3 client later means implementing this same
//! `put`/`read` shape against MinIO, not touching any caller.
//!
//! What's actually captured in this slice: full stdout+stderr from every
//! real executor invocation (Playwright/Newman/Pytest/Maestro/Schemathesis),
//! as `kind: "log"` evidence. Screenshots/video/trace capture would need
//! each tool's own artifact flags wired through (e.g. Playwright
//! `--trace=on`) — real work, deliberately left for a later pass; see
//! README.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct LocalEvidenceStore {
    root: PathBuf,
}

impl LocalEvidenceStore {
    pub fn new(root: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Writes `content` under `<root>/<run_id>/<task_id>-<kind>-<uuid>.log`
    /// and returns the object key (relative to `root`, so it's portable if
    /// `root` moves) plus the byte size actually written.
    pub fn put(
        &self,
        run_id: &str,
        task_id: &str,
        kind: &str,
        content: &str,
    ) -> std::io::Result<(String, i64)> {
        let dir = self.root.join(run_id);
        std::fs::create_dir_all(&dir)?;
        let safe_task_id: String = task_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        let file_name = format!("{safe_task_id}-{kind}-{}.log", uuid::Uuid::new_v4());
        let path = dir.join(&file_name);
        std::fs::write(&path, content.as_bytes())?;
        let object_key = format!("{run_id}/{file_name}");
        Ok((object_key, content.len() as i64))
    }

    pub fn read(&self, object_key: &str) -> std::io::Result<Vec<u8>> {
        let path = self.resolve(object_key)?;
        std::fs::read(path)
    }

    /// Rejects any object_key that would escape `root` (defense in depth —
    /// object_keys are DB-controlled, not attacker input, but a path
    /// traversal bug here would be a silent-until-exploited file read).
    fn resolve(&self, object_key: &str) -> std::io::Result<PathBuf> {
        if object_key.contains("..") {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid object_key",
            ));
        }
        let path = self.root.join(object_key);
        let root_canon = self.root.canonicalize()?;
        // The file must already exist for canonicalize to succeed; that's
        // fine, `read` should 404 on a missing key anyway.
        let path_canon = path.canonicalize()?;
        if !path_canon.starts_with(&root_canon) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "object_key escapes evidence root",
            ));
        }
        Ok(path)
    }
}

pub fn default_root() -> PathBuf {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../infra/data/evidence");
    base
}
