//! Durable, path-confined configuration transactions and recovery.

#![forbid(unsafe_code)]

use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

static TRANSACTION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fingerprint(pub String);

impl Fingerprint {
    #[must_use]
    pub fn bytes(value: &[u8]) -> Self {
        Self(blake3::hash(value).to_hex().to_string())
    }
}

#[derive(Clone, Debug)]
pub struct FileChange {
    pub target: PathBuf,
    pub expected: Option<Fingerprint>,
    pub replacement: Option<Vec<u8>>,
}

impl FileChange {
    #[must_use]
    pub fn write(
        target: impl Into<PathBuf>,
        expected: Option<Fingerprint>,
        replacement: Vec<u8>,
    ) -> Self {
        Self {
            target: target.into(),
            expected,
            replacement: Some(replacement),
        }
    }

    #[must_use]
    pub fn delete(target: impl Into<PathBuf>, expected: Fingerprint) -> Self {
        Self {
            target: target.into(),
            expected: Some(expected),
            replacement: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TransactionPlan {
    pub summary: String,
    pub changes: Vec<FileChange>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionState {
    Prepared,
    Committed,
    RolledBack,
    RecoveryConflict,
}

impl TransactionState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Committed => "committed",
            Self::RolledBack => "rolled_back",
            Self::RecoveryConflict => "recovery_conflict",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub created_ms: i64,
    pub summary: String,
    pub state: TransactionState,
    pub error: Option<String>,
    pub change_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransactionResult {
    pub id: String,
    pub state: TransactionState,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("transaction has no changes")]
    Empty,
    #[error("target is outside the allowed roots: {0}")]
    PathOutsideRoot(PathBuf),
    #[error("unsafe target type or identity: {0}")]
    UnsafeTarget(PathBuf),
    #[error("configuration changed outside Helm: {0}")]
    Conflict(PathBuf),
    #[error("verification failed: {0}")]
    Verification(String),
    #[error("rollback could not safely restore: {0}")]
    RecoveryConflict(PathBuf),
    #[error("transaction was not found: {0}")]
    NotFound(String),
    #[error("I/O failure at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("history database failure: {0}")]
    Database(#[from] rusqlite::Error),
}

pub struct Engine {
    state_root: PathBuf,
    allowed_roots: Vec<PathBuf>,
}

impl Engine {
    pub fn open(
        state_root: impl Into<PathBuf>,
        allowed_roots: Vec<PathBuf>,
    ) -> Result<Self, Error> {
        let state_root = state_root.into();
        create_private_directory(&state_root)?;
        create_private_directory(&state_root.join("snapshots"))?;
        create_private_directory(&state_root.join("locks"))?;
        let engine = Self {
            state_root,
            allowed_roots: canonical_roots(allowed_roots)?,
        };
        engine.initialize_database()?;
        engine.recover()?;
        Ok(engine)
    }

    pub fn apply<F>(&self, plan: &TransactionPlan, verify: F) -> Result<TransactionResult, Error>
    where
        F: FnOnce() -> Result<(), String>,
    {
        if plan.changes.is_empty() {
            return Err(Error::Empty);
        }
        let id = transaction_id();
        let prepared = self.prepare(&id, plan)?;
        let _locks = self.acquire_locks(&prepared)?;
        if let Err(error) = self.apply_prepared(&id, &prepared) {
            let _ = self.rollback(&id, &prepared, &error.to_string());
            return Err(error);
        }
        match verify() {
            Ok(()) => {
                self.set_state(&id, TransactionState::Committed, None)?;
                Ok(TransactionResult {
                    id,
                    state: TransactionState::Committed,
                })
            }
            Err(message) => {
                let verification = Error::Verification(message);
                self.rollback(&id, &prepared, &verification.to_string())?;
                Err(verification)
            }
        }
    }

    pub fn undo<F>(&self, transaction_id: &str, verify: F) -> Result<TransactionResult, Error>
    where
        F: FnOnce() -> Result<(), String>,
    {
        let connection = self.connection()?;
        let summary: Option<String> = connection
            .query_row(
                "SELECT summary FROM transactions WHERE id = ?1 AND state = 'committed'",
                [transaction_id],
                |row| row.get(0),
            )
            .optional()?;
        let summary = summary.ok_or_else(|| Error::NotFound(transaction_id.into()))?;
        let mut statement = connection.prepare("SELECT path, before_hash, after_hash, before_exists FROM changes WHERE transaction_id = ?1 ORDER BY ordinal")?;
        let rows = statement.query_map([transaction_id], |row| {
            Ok((
                path_from_bytes(row.get(0)?),
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })?;
        let mut changes = Vec::new();
        for row in rows {
            let (path, before_hash, after_hash, before_exists) = row?;
            let current_expected = after_hash.map(Fingerprint);
            let replacement = if before_exists {
                Some(
                    self.read_snapshot(
                        before_hash
                            .as_deref()
                            .ok_or_else(|| Error::NotFound(transaction_id.into()))?,
                    )?,
                )
            } else {
                None
            };
            changes.push(FileChange {
                target: path,
                expected: current_expected,
                replacement,
            });
        }
        drop(statement);
        drop(connection);
        self.apply(
            &TransactionPlan {
                summary: format!("Undo: {summary}"),
                changes,
            },
            verify,
        )
    }

    pub fn history(&self, limit: usize) -> Result<Vec<HistoryEntry>, Error> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT t.id,t.created_ms,t.summary,t.state,t.error,COUNT(c.ordinal) FROM transactions t LEFT JOIN changes c ON c.transaction_id=t.id GROUP BY t.id ORDER BY t.created_ms DESC,t.id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([i64::try_from(limit).unwrap_or(i64::MAX)], |row| {
            let state: String = row.get(3)?;
            Ok(HistoryEntry {
                id: row.get(0)?,
                created_ms: row.get(1)?,
                summary: row.get(2)?,
                state: parse_state(&state),
                error: row.get(4)?,
                change_count: usize::try_from(row.get::<_, i64>(5)?).unwrap_or(usize::MAX),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Error::from)
    }

    fn prepare(&self, id: &str, plan: &TransactionPlan) -> Result<Vec<PreparedChange>, Error> {
        let mut prepared = Vec::with_capacity(plan.changes.len());
        for change in &plan.changes {
            let target = self.resolve_target(&change.target)?;
            let before = read_optional(&target)?;
            let before_hash = before.as_deref().map(Fingerprint::bytes);
            if change.expected.as_ref() != before_hash.as_ref() {
                return Err(Error::Conflict(target));
            }
            if let Some(bytes) = &before {
                self.store_snapshot(bytes)?;
            }
            if let Some(bytes) = &change.replacement {
                self.store_snapshot(bytes)?;
            }
            prepared.push(PreparedChange {
                target,
                before,
                before_hash,
                replacement: change.replacement.clone(),
                after_hash: change.replacement.as_deref().map(Fingerprint::bytes),
            });
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO transactions(id,created_ms,summary,state) VALUES(?1,?2,?3,'prepared')",
            params![id, now_ms(), plan.summary],
        )?;
        for (ordinal, change) in prepared.iter().enumerate() {
            transaction.execute(
                "INSERT INTO changes(transaction_id,ordinal,path,before_hash,after_hash,before_exists,applied) VALUES(?1,?2,?3,?4,?5,?6,0)",
                params![id, i64::try_from(ordinal).unwrap_or(i64::MAX), path_bytes(&change.target), change.before_hash.as_ref().map(|hash| &hash.0), change.after_hash.as_ref().map(|hash| &hash.0), change.before.is_some()],
            )?;
        }
        transaction.commit()?;
        Ok(prepared)
    }

    fn apply_prepared(&self, id: &str, changes: &[PreparedChange]) -> Result<(), Error> {
        for (ordinal, change) in changes.iter().enumerate() {
            let current = read_optional(&change.target)?;
            if current.as_deref().map(Fingerprint::bytes).as_ref() != change.before_hash.as_ref() {
                return Err(Error::Conflict(change.target.clone()));
            }
            self.connection()?.execute(
                "UPDATE changes SET applied=1 WHERE transaction_id=?1 AND ordinal=?2",
                params![id, i64::try_from(ordinal).unwrap_or(i64::MAX)],
            )?;
            match &change.replacement {
                Some(bytes) => atomic_write(&change.target, bytes, &format!("{id}-{ordinal}"))?,
                None => remove_file_synced(&change.target)?,
            }
        }
        Ok(())
    }

    fn rollback(&self, id: &str, changes: &[PreparedChange], message: &str) -> Result<(), Error> {
        for change in changes.iter().rev() {
            let current = read_optional(&change.target)?;
            let current_hash = current.as_deref().map(Fingerprint::bytes);
            if current_hash == change.before_hash {
                continue;
            }
            if current_hash != change.after_hash {
                self.set_state(id, TransactionState::RecoveryConflict, Some(message))?;
                return Err(Error::RecoveryConflict(change.target.clone()));
            }
            match &change.before {
                Some(bytes) => atomic_write(&change.target, bytes, &format!("rollback-{id}"))?,
                None => remove_file_synced(&change.target)?,
            }
        }
        self.set_state(id, TransactionState::RolledBack, Some(message))
    }

    fn recover(&self) -> Result<(), Error> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT id FROM transactions WHERE state='prepared' ORDER BY created_ms")?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        drop(connection);
        for id in ids {
            let changes = self.load_prepared(&id)?;
            self.rollback(&id, &changes, "recovered interrupted transaction")?;
        }
        Ok(())
    }

    fn load_prepared(&self, id: &str) -> Result<Vec<PreparedChange>, Error> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT path,before_hash,after_hash,before_exists FROM changes WHERE transaction_id=?1 ORDER BY ordinal")?;
        let rows = statement.query_map([id], |row| {
            Ok((
                path_from_bytes(row.get(0)?),
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })?;
        let mut prepared = Vec::new();
        for row in rows {
            let (target, before_hash, after_hash, before_exists) = row?;
            let before = if before_exists {
                Some(
                    self.read_snapshot(
                        before_hash
                            .as_deref()
                            .ok_or_else(|| Error::NotFound(id.into()))?,
                    )?,
                )
            } else {
                None
            };
            let replacement = match &after_hash {
                Some(hash) => Some(self.read_snapshot(hash)?),
                None => None,
            };
            prepared.push(PreparedChange {
                target,
                before,
                before_hash: before_hash.map(Fingerprint),
                replacement,
                after_hash: after_hash.map(Fingerprint),
            });
        }
        Ok(prepared)
    }

    fn acquire_locks(&self, changes: &[PreparedChange]) -> Result<Vec<File>, Error> {
        changes
            .iter()
            .map(|change| {
                let name = Fingerprint::bytes(&path_bytes(&change.target)).0;
                let path = self.state_root.join("locks").join(name);
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .mode(0o600)
                    .open(&path)
                    .map_err(|source| Error::Io {
                        path: path.clone(),
                        source,
                    })?;
                file.lock().map_err(|source| Error::Io { path, source })?;
                Ok(file)
            })
            .collect()
    }

    fn resolve_target(&self, requested: &Path) -> Result<PathBuf, Error> {
        let target = if requested.exists() {
            let metadata = fs::symlink_metadata(requested).map_err(|source| Error::Io {
                path: requested.into(),
                source,
            })?;
            let resolved = if metadata.file_type().is_symlink() {
                fs::canonicalize(requested).map_err(|source| Error::Io {
                    path: requested.into(),
                    source,
                })?
            } else {
                requested.to_path_buf()
            };
            let target_metadata = fs::metadata(&resolved).map_err(|source| Error::Io {
                path: resolved.clone(),
                source,
            })?;
            if !target_metadata.is_file() || target_metadata.nlink() > 1 {
                return Err(Error::UnsafeTarget(resolved));
            }
            resolved
        } else {
            let parent = requested
                .parent()
                .ok_or_else(|| Error::UnsafeTarget(requested.into()))?;
            fs::canonicalize(parent)
                .map_err(|source| Error::Io {
                    path: parent.into(),
                    source,
                })?
                .join(
                    requested
                        .file_name()
                        .ok_or_else(|| Error::UnsafeTarget(requested.into()))?,
                )
        };
        if self
            .allowed_roots
            .iter()
            .any(|root| target.starts_with(root))
        {
            Ok(target)
        } else {
            Err(Error::PathOutsideRoot(target))
        }
    }

    fn store_snapshot(&self, bytes: &[u8]) -> Result<String, Error> {
        let hash = Fingerprint::bytes(bytes).0;
        let path = self.state_root.join("snapshots").join(&hash);
        if !path.exists() {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true).mode(0o600);
            match options.open(&path) {
                Ok(mut file) => {
                    file.write_all(bytes).map_err(|source| Error::Io {
                        path: path.clone(),
                        source,
                    })?;
                    file.sync_all().map_err(|source| Error::Io {
                        path: path.clone(),
                        source,
                    })?;
                }
                Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(source) => return Err(Error::Io { path, source }),
            }
        }
        Ok(hash)
    }

    fn read_snapshot(&self, hash: &str) -> Result<Vec<u8>, Error> {
        read_required(&self.state_root.join("snapshots").join(hash))
    }

    fn initialize_database(&self) -> Result<(), Error> {
        let connection = self.connection()?;
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS transactions(id TEXT PRIMARY KEY,created_ms INTEGER NOT NULL,summary TEXT NOT NULL,state TEXT NOT NULL,error TEXT);
            CREATE TABLE IF NOT EXISTS changes(transaction_id TEXT NOT NULL REFERENCES transactions(id),ordinal INTEGER NOT NULL,path BLOB NOT NULL,before_hash TEXT,after_hash TEXT,before_exists INTEGER NOT NULL,applied INTEGER NOT NULL,PRIMARY KEY(transaction_id,ordinal));")?;
        Ok(())
    }

    fn connection(&self) -> Result<Connection, Error> {
        let path = self.state_root.join("history.sqlite3");
        let connection = Connection::open(&path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        let permissions = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, permissions).map_err(|source| Error::Io { path, source })?;
        Ok(connection)
    }

    fn set_state(
        &self,
        id: &str,
        state: TransactionState,
        error: Option<&str>,
    ) -> Result<(), Error> {
        self.connection()?.execute(
            "UPDATE transactions SET state=?2,error=?3 WHERE id=?1",
            params![id, state.as_str(), error],
        )?;
        Ok(())
    }
}

#[derive(Clone)]
struct PreparedChange {
    target: PathBuf,
    before: Option<Vec<u8>>,
    before_hash: Option<Fingerprint>,
    replacement: Option<Vec<u8>>,
    after_hash: Option<Fingerprint>,
}

fn canonical_roots(roots: Vec<PathBuf>) -> Result<Vec<PathBuf>, Error> {
    roots
        .into_iter()
        .map(|root| fs::canonicalize(&root).map_err(|source| Error::Io { path: root, source }))
        .collect()
}

fn create_private_directory(path: &Path) -> Result<(), Error> {
    fs::create_dir_all(path).map_err(|source| Error::Io {
        path: path.into(),
        source,
    })?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| Error::Io {
        path: path.into(),
        source,
    })
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, Error> {
    if !path.exists() {
        return Ok(None);
    }
    read_required(path).map(Some)
}

fn read_required(path: &Path) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|source| Error::Io {
            path: path.into(),
            source,
        })?;
    Ok(bytes)
}

fn atomic_write(path: &Path, bytes: &[u8], suffix: &str) -> Result<(), Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::UnsafeTarget(path.into()))?;
    let temp = parent.join(format!(".helm-settings-{suffix}.tmp"));
    let existing_mode = fs::metadata(path)
        .ok()
        .map_or(0o600, |metadata| metadata.permissions().mode() & 0o777);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(existing_mode)
        .open(&temp)
        .map_err(|source| Error::Io {
            path: temp.clone(),
            source,
        })?;
    let operation = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        File::open(parent)?.sync_all()
    })();
    if let Err(source) = operation {
        let _ = fs::remove_file(&temp);
        return Err(Error::Io {
            path: path.into(),
            source,
        });
    }
    Ok(())
}

fn remove_file_synced(path: &Path) -> Result<(), Error> {
    if path.exists() {
        fs::remove_file(path).map_err(|source| Error::Io {
            path: path.into(),
            source,
        })?;
    }
    let parent = path
        .parent()
        .ok_or_else(|| Error::UnsafeTarget(path.into()))?;
    File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|source| Error::Io {
            path: parent.into(),
            source,
        })
}

fn path_bytes(path: &Path) -> Vec<u8> {
    path.as_os_str().as_bytes().to_vec()
}
fn path_from_bytes(bytes: Vec<u8>) -> PathBuf {
    PathBuf::from(OsString::from_vec(bytes))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}
fn transaction_id() -> String {
    format!(
        "{:x}-{:x}-{:x}",
        now_ms(),
        std::process::id(),
        TRANSACTION_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn parse_state(value: &str) -> TransactionState {
    match value {
        "committed" => TransactionState::Committed,
        "rolled_back" => TransactionState::RolledBack,
        "recovery_conflict" => TransactionState::RecoveryConflict,
        _ => TransactionState::Prepared,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn harness() -> (tempfile::TempDir, PathBuf, Engine) {
        let temporary = tempfile::tempdir().expect("tempdir");
        let config = temporary.path().join("config");
        fs::create_dir(&config).expect("config directory");
        let engine =
            Engine::open(temporary.path().join("state"), vec![config.clone()]).expect("engine");
        (temporary, config, engine)
    }

    #[test]
    fn applies_and_records_a_verified_change() {
        let (_temporary, config, engine) = harness();
        let target = config.join("app.toml");
        fs::write(&target, b"# keep\nvalue = 1\n").expect("fixture");
        let before = fs::read(&target).expect("read");
        let result = engine
            .apply(
                &TransactionPlan {
                    summary: "Change value".into(),
                    changes: vec![FileChange::write(
                        &target,
                        Some(Fingerprint::bytes(&before)),
                        b"# keep\nvalue = 2\n".to_vec(),
                    )],
                },
                || Ok(()),
            )
            .expect("apply");
        assert_eq!(result.state, TransactionState::Committed);
        assert_eq!(fs::read(&target).expect("read"), b"# keep\nvalue = 2\n");
        assert_eq!(engine.history(10).expect("history")[0].change_count, 1);
    }

    #[test]
    fn rejects_external_edits_without_writing() {
        let (_temporary, config, engine) = harness();
        let target = config.join("app.toml");
        fs::write(&target, b"external\n").expect("fixture");
        let result = engine.apply(
            &TransactionPlan {
                summary: "stale".into(),
                changes: vec![FileChange::write(
                    &target,
                    Some(Fingerprint::bytes(b"old\n")),
                    b"new\n".to_vec(),
                )],
            },
            || Ok(()),
        );
        assert!(matches!(result, Err(Error::Conflict(_))));
        assert_eq!(fs::read(&target).expect("read"), b"external\n");
    }

    #[test]
    fn verification_failure_restores_byte_identical_content() {
        let (_temporary, config, engine) = harness();
        let target = config.join("app.toml");
        let before = b"# formatting\r\nvalue=1\r\n".to_vec();
        fs::write(&target, &before).expect("fixture");
        let result = engine.apply(
            &TransactionPlan {
                summary: "bad reload".into(),
                changes: vec![FileChange::write(
                    &target,
                    Some(Fingerprint::bytes(&before)),
                    b"value = 2\n".to_vec(),
                )],
            },
            || Err("reload rejected".into()),
        );
        assert!(matches!(result, Err(Error::Verification(_))));
        assert_eq!(fs::read(&target).expect("read"), before);
        assert_eq!(
            engine.history(10).expect("history")[0].state,
            TransactionState::RolledBack
        );
    }

    #[test]
    fn multi_file_verification_failure_restores_every_target() {
        let (_temporary, config, engine) = harness();
        let first = config.join("first.conf");
        let second = config.join("second.conf");
        fs::write(&first, b"first-before").expect("first fixture");
        fs::write(&second, b"second-before").expect("second fixture");
        let result = engine.apply(
            &TransactionPlan {
                summary: "fault after all writes".into(),
                changes: vec![
                    FileChange::write(
                        &first,
                        Some(Fingerprint::bytes(b"first-before")),
                        b"first-after".to_vec(),
                    ),
                    FileChange::write(
                        &second,
                        Some(Fingerprint::bytes(b"second-before")),
                        b"second-after".to_vec(),
                    ),
                ],
            },
            || Err("injected runtime verification failure".into()),
        );
        assert!(matches!(result, Err(Error::Verification(_))));
        assert_eq!(fs::read(&first).expect("first restored"), b"first-before");
        assert_eq!(
            fs::read(&second).expect("second restored"),
            b"second-before"
        );
        let entry = &engine.history(1).expect("history")[0];
        assert_eq!(entry.state, TransactionState::RolledBack);
        assert_eq!(entry.change_count, 2);
    }

    #[test]
    fn undo_is_a_new_committed_transaction() {
        let (_temporary, config, engine) = harness();
        let target = config.join("app.toml");
        fs::write(&target, b"old").expect("fixture");
        let applied = engine
            .apply(
                &TransactionPlan {
                    summary: "new value".into(),
                    changes: vec![FileChange::write(
                        &target,
                        Some(Fingerprint::bytes(b"old")),
                        b"new".to_vec(),
                    )],
                },
                || Ok(()),
            )
            .expect("apply");
        let undone = engine.undo(&applied.id, || Ok(())).expect("undo");
        assert_ne!(applied.id, undone.id);
        assert_eq!(fs::read(&target).expect("read"), b"old");
        assert_eq!(engine.history(10).expect("history").len(), 2);
    }

    #[test]
    fn refuses_paths_outside_the_allowlist() {
        let (temporary, _config, engine) = harness();
        let target = temporary.path().join("outside");
        let result = engine.apply(
            &TransactionPlan {
                summary: "escape".into(),
                changes: vec![FileChange::write(target, None, b"no".to_vec())],
            },
            || Ok(()),
        );
        assert!(matches!(result, Err(Error::PathOutsideRoot(_))));
    }

    #[test]
    fn opening_engine_recovers_an_interrupted_commit() {
        let (temporary, config, engine) = harness();
        let target = config.join("app.toml");
        fs::write(&target, b"before").expect("fixture");
        let plan = TransactionPlan {
            summary: "interrupted".into(),
            changes: vec![FileChange::write(
                &target,
                Some(Fingerprint::bytes(b"before")),
                b"after".to_vec(),
            )],
        };
        let prepared = engine.prepare("crash-fixture", &plan).expect("prepare");
        engine
            .apply_prepared("crash-fixture", &prepared)
            .expect("write");
        drop(engine);
        let recovered =
            Engine::open(temporary.path().join("state"), vec![config]).expect("recover engine");
        assert_eq!(fs::read(&target).expect("read"), b"before");
        assert_eq!(
            recovered.history(10).expect("history")[0].state,
            TransactionState::RolledBack
        );
    }

    #[test]
    fn writing_through_a_safe_symlink_preserves_the_link() {
        use std::os::unix::fs::symlink;

        let (_temporary, config, engine) = harness();
        let target = config.join("real.toml");
        let link = config.join("app.toml");
        fs::write(&target, b"before").expect("fixture");
        symlink(&target, &link).expect("symlink");
        engine
            .apply(
                &TransactionPlan {
                    summary: "symlink target".into(),
                    changes: vec![FileChange::write(
                        &link,
                        Some(Fingerprint::bytes(b"before")),
                        b"after".to_vec(),
                    )],
                },
                || Ok(()),
            )
            .expect("apply");
        assert!(
            fs::symlink_metadata(&link)
                .expect("metadata")
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(target).expect("read"), b"after");
    }
}
