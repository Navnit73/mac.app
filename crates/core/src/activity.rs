//! History of cleanups: every batch moved to the Trash, what happened to each item since, and
//! the weekly totals shown in the Activity room. Stored in SQLite (WAL) on this Mac only.
//!
//! Paths are stored as raw bytes so a restore puts a file back under exactly the name it had.

use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

use crate::errors::AppError;
use crate::models::{
    BatchItem, BatchPage, BatchSummary, ItemStatus, Room, SkippedItem, TrashBatchResult,
    UndoResult, WeekStat, WeeklyStats,
};
use crate::trash::{self, MoveOutcome, Restore};

const SCHEMA_VERSION: i64 = 1;
const WEEK: i64 = 7 * 86_400;
pub const ITEM_PAGE: usize = 200;

fn db_err(e: rusqlite::Error) -> AppError {
    AppError::Internal(format!("activity database: {e}"))
}

fn to_blob(p: &Path) -> Vec<u8> {
    p.as_os_str().as_bytes().to_vec()
}

fn from_blob(b: Vec<u8>) -> PathBuf {
    PathBuf::from(std::ffi::OsString::from_vec(b))
}

fn lossy(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// Monday 00:00 UTC of the week containing `t` (Unix seconds). 1970-01-01 was a Thursday.
pub fn week_start(t: i64) -> i64 {
    let day = t.div_euclid(86_400);
    let monday = day - (day + 3).rem_euclid(7);
    monday * 86_400
}

/// Monday 00:00 local time (given the UTC offset in seconds) of the week containing `t`.
pub fn local_week_start(t: i64, utc_offset: i64) -> i64 {
    week_start(t + utc_offset) - utc_offset
}

/// `YYYY-MM-DD HH:MM:SS` in UTC.
pub fn iso_utc(t: i64) -> String {
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    // Civil date from days since epoch (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (and create or migrate) the database at `path`.
    pub fn open(path: &Path) -> Result<Self, AppError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path).map_err(db_err)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS batches (
                 id INTEGER PRIMARY KEY,
                 created_at INTEGER NOT NULL,
                 room TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS batch_items (
                 id INTEGER PRIMARY KEY,
                 batch_id INTEGER NOT NULL REFERENCES batches(id) ON DELETE CASCADE,
                 original_path BLOB NOT NULL,
                 trashed_path BLOB,
                 size INTEGER NOT NULL,
                 status TEXT NOT NULL,
                 reason TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_items_batch ON batch_items(batch_id);
             CREATE INDEX IF NOT EXISTS idx_items_status ON batch_items(status);",
        )
        .map_err(db_err)?;
        conn.execute(
            "INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )
        .map_err(db_err)?;
        Ok(Self { conn })
    }

    /// Record a finished move. Nothing is written when nothing moved.
    pub fn record(
        &mut self,
        room: Room,
        out: &MoveOutcome,
        now: i64,
    ) -> Result<TrashBatchResult, AppError> {
        let skipped: Vec<SkippedItem> = out
            .skipped
            .iter()
            .map(|(p, _, r)| SkippedItem {
                path: lossy(p),
                reason: r.clone(),
            })
            .collect();
        let moved_bytes: u64 = out.moved.iter().map(|m| m.size).sum();
        if out.moved.is_empty() {
            return Ok(TrashBatchResult {
                batch_id: None,
                moved_count: 0,
                moved_bytes: 0,
                skipped,
            });
        }
        let tx = self.conn.transaction().map_err(db_err)?;
        tx.execute(
            "INSERT INTO batches (created_at, room) VALUES (?1, ?2)",
            params![now, room.as_str()],
        )
        .map_err(db_err)?;
        let batch_id = tx.last_insert_rowid();
        {
            let mut ins = tx
                .prepare(
                    "INSERT INTO batch_items (batch_id, original_path, trashed_path, size, status, reason)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .map_err(db_err)?;
            for m in &out.moved {
                ins.execute(params![
                    batch_id,
                    to_blob(&m.original),
                    to_blob(&m.trashed),
                    m.size as i64,
                    ItemStatus::InTrash.as_str(),
                    Option::<String>::None
                ])
                .map_err(db_err)?;
            }
            for (p, size, reason) in &out.skipped {
                ins.execute(params![
                    batch_id,
                    to_blob(p),
                    Option::<Vec<u8>>::None,
                    *size as i64,
                    ItemStatus::Skipped.as_str(),
                    reason
                ])
                .map_err(db_err)?;
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(TrashBatchResult {
            batch_id: Some(batch_id),
            moved_count: out.moved.len() as u32,
            moved_bytes,
            skipped,
        })
    }

    fn summary_row(&self, id: i64) -> Result<Option<BatchSummary>, AppError> {
        self.conn
            .query_row(
                "SELECT b.id, b.created_at, b.room,
                    SUM(CASE WHEN i.status != 'skipped' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN i.status IN ('in_trash','emptied','conflict') THEN i.size ELSE 0 END),
                    SUM(i.status = 'in_trash'), SUM(i.status = 'restored'),
                    SUM(i.status = 'emptied'), SUM(i.status = 'conflict')
                 FROM batches b JOIN batch_items i ON i.batch_id = b.id
                 WHERE b.id = ?1 GROUP BY b.id",
                params![id],
                Self::map_summary,
            )
            .optional()
            .map_err(db_err)
    }

    fn map_summary(r: &rusqlite::Row<'_>) -> rusqlite::Result<BatchSummary> {
        let room: String = r.get(2)?;
        Ok(BatchSummary {
            id: r.get(0)?,
            created_at: r.get(1)?,
            room: Room::parse(&room),
            item_count: r.get::<_, i64>(3)? as u32,
            bytes: r.get::<_, i64>(4)?.max(0) as u64,
            in_trash: r.get::<_, i64>(5)? as u32,
            restored: r.get::<_, i64>(6)? as u32,
            emptied: r.get::<_, i64>(7)? as u32,
            conflicts: r.get::<_, i64>(8)? as u32,
        })
    }

    /// Newest first.
    pub fn list_batches(&self, limit: u32, offset: u32) -> Result<Vec<BatchSummary>, AppError> {
        self.batches(i64::from(limit.min(500)), offset)
    }

    /// Batch summaries, newest first. `limit` -1 means all (for totals, never sent over IPC).
    fn batches(&self, limit: i64, offset: u32) -> Result<Vec<BatchSummary>, AppError> {
        let mut st = self
            .conn
            .prepare(
                "SELECT b.id, b.created_at, b.room,
                    SUM(CASE WHEN i.status != 'skipped' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN i.status IN ('in_trash','emptied','conflict') THEN i.size ELSE 0 END),
                    SUM(i.status = 'in_trash'), SUM(i.status = 'restored'),
                    SUM(i.status = 'emptied'), SUM(i.status = 'conflict')
                 FROM batches b JOIN batch_items i ON i.batch_id = b.id
                 GROUP BY b.id ORDER BY b.created_at DESC, b.id DESC LIMIT ?1 OFFSET ?2",
            )
            .map_err(db_err)?;
        let rows = st
            .query_map(params![limit, offset], Self::map_summary)
            .map_err(db_err)?;
        rows.collect::<Result<_, _>>().map_err(db_err)
    }

    pub fn get_batch(&self, id: i64, page: u32) -> Result<BatchPage, AppError> {
        let batch = self
            .summary_row(id)?
            .ok_or_else(|| AppError::NotFound(format!("batch {id}")))?;
        let total: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM batch_items WHERE batch_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        let offset = (page as usize) * ITEM_PAGE;
        let mut st = self
            .conn
            .prepare(
                "SELECT id, original_path, trashed_path, size, status, reason FROM batch_items
                 WHERE batch_id = ?1 ORDER BY size DESC, id LIMIT ?2 OFFSET ?3",
            )
            .map_err(db_err)?;
        let items = st
            .query_map(params![id, ITEM_PAGE as i64, offset as i64], |r| {
                let status: String = r.get(4)?;
                Ok(BatchItem {
                    id: r.get(0)?,
                    original_path: lossy(&from_blob(r.get(1)?)),
                    trashed_path: r
                        .get::<_, Option<Vec<u8>>>(2)?
                        .map(|b| lossy(&from_blob(b))),
                    size: r.get::<_, i64>(3)?.max(0) as u64,
                    status: ItemStatus::parse(&status),
                    reason: r.get(5)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        let end = offset + items.len();
        Ok(BatchPage {
            batch,
            items,
            total: total as u32,
            next_page: ((end as i64) < total).then_some(page + 1),
        })
    }

    /// The most recent batch that still has something in the Trash (for ⌘Z).
    pub fn last_undoable(&self) -> Result<Option<i64>, AppError> {
        self.conn
            .query_row(
                "SELECT b.id FROM batches b JOIN batch_items i ON i.batch_id = b.id
                 WHERE i.status IN ('in_trash','conflict')
                 ORDER BY b.created_at DESC, b.id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)
    }

    /// Restore a batch (or chosen items of it) from the Trash. Returns the restored paths too,
    /// so callers can refresh the scan.
    pub fn undo(
        &mut self,
        batch_id: i64,
        item_ids: Option<&[i64]>,
        allow_rename: bool,
    ) -> Result<(UndoResult, Vec<PathBuf>), AppError> {
        let rows: Vec<(i64, PathBuf, Option<PathBuf>)> = {
            let mut st = self
                .conn
                .prepare(
                    "SELECT id, original_path, trashed_path FROM batch_items
                     WHERE batch_id = ?1 AND status IN ('in_trash','conflict')",
                )
                .map_err(db_err)?;
            let mapped = st
                .query_map(params![batch_id], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        from_blob(r.get(1)?),
                        r.get::<_, Option<Vec<u8>>>(2)?.map(from_blob),
                    ))
                })
                .map_err(db_err)?;
            mapped.collect::<Result<_, _>>().map_err(db_err)?
        };
        let wanted = |id: i64| item_ids.is_none_or(|ids| ids.contains(&id));

        let mut result = UndoResult::default();
        let mut restored_paths = Vec::new();
        let tx = self.conn.transaction().map_err(db_err)?;
        for (id, original, trashed) in rows.into_iter().filter(|r| wanted(r.0)) {
            let Some(trashed) = trashed else { continue };
            let (status, reason) = match trash::restore(&trashed, &original, allow_rename) {
                Restore::Restored(p) => {
                    result.restored += 1;
                    restored_paths.push(p);
                    (ItemStatus::Restored, None)
                }
                Restore::Renamed(p) => {
                    result.restored += 1;
                    result.renamed += 1;
                    let note = format!("Restored as {}", lossy(&p));
                    restored_paths.push(p);
                    (ItemStatus::Restored, Some(note))
                }
                Restore::Conflict => {
                    result.conflicts += 1;
                    (
                        ItemStatus::Conflict,
                        Some("Something now exists at the original location.".to_string()),
                    )
                }
                Restore::Missing => {
                    result.missing += 1;
                    (ItemStatus::Emptied, None)
                }
                Restore::Failed(reason) => {
                    result.failed.push(SkippedItem {
                        path: lossy(&original),
                        reason: reason.clone(),
                    });
                    // Leave it recorded as still in the Trash so it can be retried.
                    (ItemStatus::InTrash, Some(reason))
                }
            };
            tx.execute(
                "UPDATE batch_items SET status = ?1, reason = ?2 WHERE id = ?3",
                params![status.as_str(), reason, id],
            )
            .map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        Ok((result, restored_paths))
    }

    /// Items MyMacStorage moved that are still in the Trash: (count, bytes).
    pub fn in_trash_totals(&self) -> Result<(u32, u64), AppError> {
        self.conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(size), 0) FROM batch_items WHERE status IN ('in_trash','conflict')",
                [],
                |r| Ok((r.get::<_, i64>(0)? as u32, r.get::<_, i64>(1)?.max(0) as u64)),
            )
            .map_err(db_err)
    }

    /// Mark items whose Trash copy is gone as emptied. Returns how many changed.
    pub fn reconcile(&mut self) -> Result<u32, AppError> {
        let rows: Vec<(i64, PathBuf)> = {
            let mut st = self
                .conn
                .prepare(
                    "SELECT id, trashed_path FROM batch_items
                     WHERE status IN ('in_trash','conflict') AND trashed_path IS NOT NULL",
                )
                .map_err(db_err)?;
            let mapped = st
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, from_blob(r.get(1)?))))
                .map_err(db_err)?;
            mapped.collect::<Result<_, _>>().map_err(db_err)?
        };
        let mut changed = 0;
        let tx = self.conn.transaction().map_err(db_err)?;
        for (id, path) in rows {
            match std::fs::symlink_metadata(&path) {
                Ok(_) => {}
                // Without Full Disk Access the Trash is unreadable; that says nothing about
                // whether the item is gone, so leave it alone.
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {}
                Err(_) => {
                    tx.execute(
                        "UPDATE batch_items SET status = 'emptied' WHERE id = ?1",
                        params![id],
                    )
                    .map_err(db_err)?;
                    changed += 1;
                }
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(changed)
    }

    /// After an Empty Trash that succeeded, every item MyMacStorage put there is gone.
    pub fn mark_all_emptied(&mut self) -> Result<u32, AppError> {
        self.conn
            .execute(
                "UPDATE batch_items SET status = 'emptied' WHERE status IN ('in_trash','conflict')",
                [],
            )
            .map(|n| n as u32)
            .map_err(db_err)
    }

    /// Recovered bytes per week for the last `weeks` weeks (oldest first), plus totals over every
    /// batch. Weeks start on Monday in local time (`utc_offset` seconds east of UTC).
    pub fn weekly_stats(
        &self,
        weeks: u32,
        now: i64,
        utc_offset: i64,
    ) -> Result<WeeklyStats, AppError> {
        let weeks = weeks.clamp(1, 104) as i64;
        let this_week = local_week_start(now, utc_offset);
        let first = this_week - (weeks - 1) * WEEK;
        let summaries = self.batches(-1, 0)?;
        let mut out: Vec<WeekStat> = (0..weeks)
            .map(|i| WeekStat {
                week_start: first + i * WEEK,
                bytes: 0,
                batches: 0,
            })
            .collect();
        for b in &summaries {
            let w = local_week_start(b.created_at, utc_offset);
            if w >= first {
                if let Some(slot) = out.get_mut(((w - first) / WEEK) as usize) {
                    slot.bytes += b.bytes;
                    slot.batches += 1;
                }
            }
        }
        Ok(WeeklyStats {
            this_week_bytes: out.last().map_or(0, |w| w.bytes),
            weeks: out,
            total_bytes: summaries.iter().map(|b| b.bytes).sum(),
            total_batches: summaries.len() as u32,
            biggest_bytes: summaries.iter().map(|b| b.bytes).max().unwrap_or(0),
        })
    }

    /// Every item of every batch as CSV text.
    pub fn csv(&self) -> Result<String, AppError> {
        let mut st = self
            .conn
            .prepare(
                "SELECT b.id, b.created_at, b.room, i.original_path, i.size, i.status, i.reason
                 FROM batches b JOIN batch_items i ON i.batch_id = b.id
                 ORDER BY b.created_at, b.id, i.id",
            )
            .map_err(db_err)?;
        let mut out = String::from("batch,date_utc,room,path,size_bytes,status,note\n");
        let rows = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    from_blob(r.get(3)?),
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, Option<String>>(6)?,
                ))
            })
            .map_err(db_err)?;
        for row in rows {
            let (id, at, room, path, size, status, note) = row.map_err(db_err)?;
            out.push_str(&format!(
                "{id},{},{},{},{size},{},{}\n",
                iso_utc(at),
                csv_field(&room),
                csv_field(&lossy(&path)),
                csv_field(&status),
                csv_field(note.as_deref().unwrap_or(""))
            ));
        }
        Ok(out)
    }

    /// Forget the history. Files are never touched.
    pub fn clear(&mut self) -> Result<(), AppError> {
        self.conn
            .execute_batch("DELETE FROM batch_items; DELETE FROM batches;")
            .map_err(db_err)
    }
}

/// Write `contents` to `dir/name.csv`, adding " 2", " 3"... instead of overwriting.
pub fn write_new_file(dir: &Path, stem: &str, contents: &str) -> Result<PathBuf, AppError> {
    std::fs::create_dir_all(dir)?;
    for n in 1..1000 {
        let name = if n == 1 {
            format!("{stem}.csv")
        } else {
            format!("{stem} {n}.csv")
        };
        let path = dir.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                use std::io::Write;
                f.write_all(contents.as_bytes())?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(AppError::Io("could not find a free file name".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trash::{move_items, Candidate, Context, Expected, FakeTrash};
    use std::io::Write;
    use std::os::macos::fs::MetadataExt;

    struct Env {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        ctx: Context,
        trash: FakeTrash,
        store: Store,
    }

    fn env() -> Env {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let root = base.join("scan");
        std::fs::create_dir_all(&root).unwrap();
        Env {
            store: Store::open(&base.join("db/activity.sqlite")).unwrap(),
            trash: FakeTrash::new(&base).unwrap(),
            ctx: Context {
                roots: vec![root.clone()],
                app_bundle: None,
            },
            root,
            _tmp: tmp,
        }
    }

    fn make(e: &Env, name: &str, len: usize) -> (Candidate, u64) {
        let f = e.root.join(name);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::File::create(&f)
            .unwrap()
            .write_all(&vec![b'x'; len])
            .unwrap();
        let m = std::fs::symlink_metadata(&f).unwrap();
        (
            Candidate {
                path: f,
                expected: Expected {
                    dev: m.st_dev() as u32,
                    inode: m.st_ino(),
                    size: m.st_size(),
                    mtime: m.st_mtime(),
                    is_dir: false,
                },
            },
            len as u64,
        )
    }

    #[test]
    fn dates_and_weeks() {
        assert_eq!(iso_utc(0), "1970-01-01 00:00:00");
        assert_eq!(iso_utc(951_868_800 + 3661), "2000-03-01 01:01:01");
        // 1970-01-05 was a Monday.
        assert_eq!(week_start(4 * 86_400), 4 * 86_400);
        assert_eq!(week_start(4 * 86_400 - 1), -3 * 86_400);
        assert_eq!(week_start(10 * 86_400 + 5), 4 * 86_400);
        // Monday 1970-01-05 03:00 at UTC+5:30 is Sunday 21:30 UTC: still Monday's week locally.
        let ist = 5 * 3600 + 1800;
        let t = 4 * 86_400 + 3 * 3600 - ist;
        assert_eq!(week_start(t), -3 * 86_400);
        assert_eq!(local_week_start(t, ist), 4 * 86_400 - ist);
    }

    #[test]
    fn csv_escaping() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn record_undo_and_stats() {
        let mut e = env();
        let a = make(&e, "a.bin", 1000);
        let b = make(&e, "b.bin", 3000);
        let out = move_items(&[a, b], &e.ctx, &e.trash);
        let res = e.store.record(Room::Find, &out, 10 * 86_400).unwrap();
        let id = res.batch_id.unwrap();
        assert_eq!((res.moved_count, res.moved_bytes), (2, 4000));

        let list = e.store.list_batches(10, 0).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(
            (list[0].bytes, list[0].in_trash, list[0].room),
            (4000, 2, Room::Find)
        );
        assert_eq!(e.store.last_undoable().unwrap(), Some(id));

        let page = e.store.get_batch(id, 0).unwrap();
        assert_eq!(page.items[0].size, 3000, "largest first");
        let first_item = page.items[0].id;

        // Undo one item, then the rest.
        let (r, paths) = e.store.undo(id, Some(&[first_item]), false).unwrap();
        assert_eq!(r.restored, 1);
        assert_eq!(paths, vec![e.root.join("b.bin")]);
        assert_eq!(e.store.get_batch(id, 0).unwrap().batch.bytes, 1000);
        let (r, _) = e.store.undo(id, None, false).unwrap();
        assert_eq!(r.restored, 1);
        assert!(e.root.join("a.bin").exists() && e.root.join("b.bin").exists());
        assert_eq!(e.store.last_undoable().unwrap(), None);

        let stats = e.store.weekly_stats(4, 12 * 86_400, 0).unwrap();
        assert_eq!(
            stats.total_bytes, 0,
            "fully restored batches recover nothing"
        );
        assert_eq!(stats.total_batches, 1);
    }

    #[test]
    fn weekly_totals_equal_sum_of_batches() {
        let mut e = env();
        let mut expected = 0;
        for (i, day) in [0i64, 1, 8, 15, 15].iter().enumerate() {
            let c = make(&e, &format!("f{i}"), 100 * (i + 1));
            expected += 100 * (i as u64 + 1);
            let out = move_items(&[c], &e.ctx, &e.trash);
            e.store
                .record(Room::Space, &out, 100 * 86_400 + day * 86_400)
                .unwrap();
        }
        let stats = e.store.weekly_stats(8, 116 * 86_400, 0).unwrap();
        let sum: u64 = stats.weeks.iter().map(|w| w.bytes).sum();
        assert_eq!(sum, expected);
        assert_eq!(stats.total_bytes, expected);
        assert_eq!(stats.total_batches, 5);
        assert_eq!(stats.biggest_bytes, 500);
        assert_eq!(stats.weeks.len(), 8);
    }

    #[test]
    fn skipped_items_are_recorded_but_not_counted() {
        let mut e = env();
        let a = make(&e, "a", 10);
        let mut b = make(&e, "b", 20);
        b.0.expected.size = 999; // pretend it changed
        let out = move_items(&[a, b], &e.ctx, &e.trash);
        let res = e.store.record(Room::Inspector, &out, 0).unwrap();
        assert_eq!(res.skipped.len(), 1);
        let page = e.store.get_batch(res.batch_id.unwrap(), 0).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.batch.item_count, 1);
        assert_eq!(page.batch.bytes, 10);
    }

    #[test]
    fn nothing_moved_records_nothing() {
        let mut e = env();
        let res = e
            .store
            .record(Room::Space, &MoveOutcome::default(), 0)
            .unwrap();
        assert!(res.batch_id.is_none());
        assert!(e.store.list_batches(10, 0).unwrap().is_empty());
    }

    #[test]
    fn reconcile_marks_emptied() {
        let mut e = env();
        let out = move_items(&[make(&e, "a", 10), make(&e, "b", 10)], &e.ctx, &e.trash);
        let id = e
            .store
            .record(Room::Space, &out, 0)
            .unwrap()
            .batch_id
            .unwrap();
        std::fs::remove_file(&out.moved[0].trashed).unwrap();
        assert_eq!(e.store.reconcile().unwrap(), 1);
        let b = e.store.get_batch(id, 0).unwrap().batch;
        assert_eq!((b.emptied, b.in_trash), (1, 1));
        assert_eq!(e.store.in_trash_totals().unwrap(), (1, 10));
        assert_eq!(e.store.mark_all_emptied().unwrap(), 1);
        assert_eq!(e.store.in_trash_totals().unwrap(), (0, 0));
        // Emptied items still count as recovered space.
        assert_eq!(e.store.weekly_stats(1, 0, 0).unwrap().total_bytes, 20);
    }

    #[test]
    fn conflict_is_kept_and_suffix_restore_works() {
        let mut e = env();
        let out = move_items(&[make(&e, "doc.txt", 5)], &e.ctx, &e.trash);
        let id = e
            .store
            .record(Room::Space, &out, 0)
            .unwrap()
            .batch_id
            .unwrap();
        std::fs::write(e.root.join("doc.txt"), b"new").unwrap();
        let (r, _) = e.store.undo(id, None, false).unwrap();
        assert_eq!(r.conflicts, 1);
        assert_eq!(e.store.get_batch(id, 0).unwrap().batch.conflicts, 1);
        let (r, paths) = e.store.undo(id, None, true).unwrap();
        assert_eq!((r.restored, r.renamed), (1, 1));
        assert_eq!(paths[0], e.root.join("doc (restored).txt"));
    }

    #[test]
    fn clear_history_leaves_files_alone() {
        let mut e = env();
        let out = move_items(&[make(&e, "a", 10)], &e.ctx, &e.trash);
        e.store.record(Room::Space, &out, 0).unwrap();
        e.store.clear().unwrap();
        assert!(e.store.list_batches(10, 0).unwrap().is_empty());
        assert!(out.moved[0].trashed.exists(), "the file stays in the Trash");
    }

    #[test]
    fn csv_export_and_no_overwrite() {
        let mut e = env();
        let out = move_items(&[make(&e, "name, with comma.txt", 10)], &e.ctx, &e.trash);
        e.store.record(Room::Find, &out, 0).unwrap();
        let csv = e.store.csv().unwrap();
        assert!(csv.starts_with("batch,date_utc,room,path"));
        assert!(csv.contains("\"") && csv.contains("in_trash"));
        let dir = e.root.join("out");
        let a = write_new_file(&dir, "export", &csv).unwrap();
        let b = write_new_file(&dir, "export", "second").unwrap();
        assert_ne!(a, b);
        assert_eq!(std::fs::read_to_string(&a).unwrap(), csv);
    }

    #[test]
    fn database_survives_reopen() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("a.sqlite");
        let e = env();
        {
            let mut s = Store::open(&db).unwrap();
            let out = move_items(&[make(&e, "a", 10)], &e.ctx, &e.trash);
            s.record(Room::Space, &out, 0).unwrap();
        }
        let s = Store::open(&db).unwrap();
        assert_eq!(s.list_batches(10, 0).unwrap().len(), 1);
    }
}
