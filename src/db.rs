use rusqlite::{Connection, Result, params};

#[derive(Debug)]
pub struct Transfer {
    pub id: i64,
    pub path: String,
    pub ip: String,
    pub port: u16,
    pub status: String,
    pub created_at: String,
    pub listing_complete: bool,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct FileRecord {
    pub id: i64,
    pub relative_path: String,
    pub size: u64,
    pub is_dir: bool,
    pub status: String,
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn init() -> Result<Self> {
        let conn = Connection::open("send_history.db")?;
        // history table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL,
                ip TEXT NOT NULL,
                port INTEGER NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                listing_complete BOOLEAN DEFAULT 0
            )",
            [],
        )?;
        // Ensure column exists if table already existed (simple migration check)
        // Ignoring properly migration for simplicity, assuming user can accept schema changes or fresh start if needed.
        // But to be safe, we can try to add the column and ignore error.
        let _ = conn.execute(
            "ALTER TABLE history ADD COLUMN listing_complete BOOLEAN DEFAULT 0",
            [],
        );
        // Optimize performance
        let _: String = conn.query_row("PRAGMA journal_mode=WAL;", [], |row| row.get(0))?;
        conn.execute("PRAGMA synchronous=NORMAL;", [])?;

        Ok(Db { conn })
    }

    pub fn add_transfer(&self, path: &str, ip: &str, port: u16) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO history (path, ip, port, status, listing_complete) VALUES (?1, ?2, ?3, ?4, 0)",
            params![path, ip, port, "Pending"],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_status(&self, id: i64, status: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE history SET status = ?2 WHERE id = ?1",
            params![id, status],
        )?;
        Ok(())
    }

    pub fn set_listing_complete(&self, id: i64, complete: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE history SET listing_complete = ?2 WHERE id = ?1",
            params![id, complete],
        )?;
        Ok(())
    }

    pub fn delete_transfer(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn list_transfers(&self) -> Result<Vec<Transfer>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, ip, port, status, created_at, listing_complete FROM history ORDER BY id DESC",
        )?;
        let transfer_iter = stmt.query_map([], |row| {
            Ok(Transfer {
                id: row.get(0)?,
                path: row.get(1)?,
                ip: row.get(2)?,
                port: row.get(3)?,
                status: row.get(4)?,
                created_at: row.get(5)?,
                listing_complete: row.get(6)?,
            })
        })?;

        let mut transfers = Vec::new();
        for transfer in transfer_iter {
            transfers.push(transfer?);
        }
        Ok(transfers)
    }

    pub fn get_transfer(&self, id: i64) -> Result<Transfer> {
        self.conn.query_row(
            "SELECT id, path, ip, port, status, created_at, listing_complete FROM history WHERE id = ?1",
            params![id],
            |row| {
                Ok(Transfer {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    ip: row.get(2)?,
                    port: row.get(3)?,
                    status: row.get(4)?,
                    created_at: row.get(5)?,
                    listing_complete: row.get(6)?,
                })
            },
        )
    }
}

pub struct TransferLog {
    conn: Connection,
}

impl TransferLog {
    pub fn new(transfer_id: i64) -> Result<Self> {
        let db_path = format!("send_history_{}.db", transfer_id);
        let conn = Connection::open(db_path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS files (
                id INTEGER PRIMARY KEY,
                relative_path TEXT UNIQUE NOT NULL,
                size INTEGER NOT NULL,
                is_dir BOOLEAN NOT NULL,
                status TEXT NOT NULL DEFAULT 'Pending'
            )",
            [],
        )?;

        // Optimize performance for this log DB too
        let _: String = conn.query_row("PRAGMA journal_mode=WAL;", [], |row| row.get(0))?;
        conn.execute("PRAGMA synchronous=NORMAL;", [])?;

        Ok(TransferLog { conn })
    }

    pub fn reset(&self) -> Result<()> {
        self.conn.execute("DELETE FROM files", [])?;
        Ok(())
    }

    pub fn add_file(&self, relative_path: &str, size: u64, is_dir: bool) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO files (relative_path, size, is_dir, status) VALUES (?1, ?2, ?3, 'Pending')",
            params![relative_path, size, is_dir],
        )?;
        Ok(())
    }

    pub fn mark_sent(&self, relative_path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE files SET status = 'Sent' WHERE relative_path = ?1",
            params![relative_path],
        )?;
        Ok(())
    }

    pub fn mark_skipped(&self, relative_path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE files SET status = 'Skipped' WHERE relative_path = ?1",
            params![relative_path],
        )?;
        Ok(())
    }

    pub fn get_pending_files(&self) -> Result<Vec<FileRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, relative_path, size, is_dir, status FROM files WHERE status = 'Pending'",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(FileRecord {
                id: row.get(0)?,
                relative_path: row.get(1)?,
                size: row.get(2)?,
                is_dir: row.get(3)?,
                status: row.get(4)?,
            })
        })?;

        let mut files = Vec::new();
        for r in rows {
            files.push(r?);
        }
        Ok(files)
    }

    pub fn count_pending(&self) -> Result<u64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM files WHERE status = 'Pending'",
            [],
            |row| row.get(0),
        )
    }

    pub fn count_total(&self) -> Result<u64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
    }

    pub fn count_skipped(&self) -> Result<u64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM files WHERE status = 'Skipped'",
            [],
            |row| row.get(0),
        )
    }

    pub fn get_total_sent_bytes(&self) -> Result<u64> {
        // Sum size of all files with status = 'Sent'
        self.conn.query_row(
            "SELECT COALESCE(SUM(size), 0) FROM files WHERE status = 'Sent'",
            [],
            |row| row.get(0),
        )
    }
}
