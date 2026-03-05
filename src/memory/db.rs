use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;
use tokio_rusqlite::Connection as AsyncConnection;

#[allow(dead_code)]
pub struct Db {
    pub conn: AsyncConnection,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct TopicState {
    pub id: String,
    pub summary: String,
    pub last_summarized_log_id: i64,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct AuditLog {
    pub id: i64,
    pub role: String,
    pub content: String,
}

#[allow(dead_code)]
impl Db {
    /// Opens the database at the given path and runs schema initialization.
    pub async fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_owned();
        let conn = AsyncConnection::open(path).await?;

        conn.call(|conn| Ok(Self::init_schema(conn)?)).await?;

        Ok(Self { conn })
    }

    /// Opens an in-memory database for testing.
    pub async fn open_in_memory() -> Result<Self> {
        let conn = AsyncConnection::open_in_memory().await?;

        conn.call(|conn| Ok(Self::init_schema(conn)?)).await?;
        Ok(Self { conn })
    }

    pub async fn create_topic(&self, topic_id: &str, channel_id: &str) -> Result<()> {
        let topic_id = topic_id.to_owned();
        let channel_id = channel_id.to_owned();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR IGNORE INTO topics (id, channel_id, topic_summary, last_summarized_log_id) VALUES (?1, ?2, '', 0)",
                    rusqlite::params![topic_id, channel_id],
                )?;
                Ok(())
            })
            .await?;

        Ok(())
    }
    pub async fn insert_audit_log(
        &self,
        channel_id: &str,
        topic_id: &str,
        role: &str,
        content: &str,
        artifact_paths: &str,
    ) -> Result<i64> {
        let channel_id = channel_id.to_owned();
        let topic_id = topic_id.to_owned();
        let role = role.to_owned();
        let content = content.to_owned();
        let artifact_paths = artifact_paths.to_owned();

        let id = self
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO audit_logs (channel_id, topic_id, role, content, artifact_paths)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![channel_id, topic_id, role, content, artifact_paths],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await?;

        Ok(id)
    }

    pub async fn fetch_topic_state(&self, topic_id: &str) -> Result<Option<TopicState>> {
        let topic_id = topic_id.to_owned();
        let res = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, topic_summary, last_summarized_log_id FROM topics WHERE id = ?1",
                )?;
                let mut rows = stmt.query(rusqlite::params![topic_id])?;
                if let Some(row) = rows.next()? {
                    Ok(Some(TopicState {
                        id: row.get(0)?,
                        summary: row.get(1)?,
                        last_summarized_log_id: row.get(2)?,
                    }))
                } else {
                    Ok(None)
                }
            })
            .await?;
        Ok(res)
    }

    pub async fn fetch_unsummarized_logs(
        &self,
        topic_id: &str,
        last_summarized_log_id: i64,
    ) -> Result<Vec<AuditLog>> {
        let topic_id = topic_id.to_owned();
        let logs = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare("SELECT id, role, content FROM audit_logs WHERE topic_id = ?1 AND id > ?2 ORDER BY id ASC")?;
                let rows = stmt.query_map(rusqlite::params![topic_id, last_summarized_log_id], |row| {
                    Ok(AuditLog {
                        id: row.get(0)?,
                        role: row.get(1)?,
                        content: row.get(2)?,
                    })
                })?;
                let mut result = Vec::new();
                for row in rows {
                    result.push(row?);
                }
                Ok(result)
            })
            .await?;
        Ok(logs)
    }
    pub async fn count_unsummarized_logs(
        &self,
        topic_id: &str,
        last_summarized_log_id: i64,
    ) -> Result<i64> {
        let topic_id = topic_id.to_owned();
        let count = self
            .conn
            .call(move |conn| {
                let mut stmt = conn
                    .prepare("SELECT count(*) FROM audit_logs WHERE topic_id = ?1 AND id > ?2")?;
                let count: i64 = stmt
                    .query_row(rusqlite::params![topic_id, last_summarized_log_id], |row| {
                        row.get(0)
                    })?;
                Ok(count)
            })
            .await?;
        Ok(count)
    }

    pub async fn fetch_oldest_unsummarized_logs(
        &self,
        topic_id: &str,
        last_summarized_log_id: i64,
        limit: i64,
    ) -> Result<Vec<AuditLog>> {
        let topic_id = topic_id.to_owned();
        let logs = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare("SELECT id, role, content FROM audit_logs WHERE topic_id = ?1 AND id > ?2 ORDER BY id ASC LIMIT ?3")?;
                let rows = stmt.query_map(rusqlite::params![topic_id, last_summarized_log_id, limit], |row| {
                    Ok(AuditLog {
                        id: row.get(0)?,
                        role: row.get(1)?,
                        content: row.get(2)?,
                    })
                })?;
                let mut result = Vec::new();
                for row in rows {
                    result.push(row?);
                }
                Ok(result)
            })
            .await?;
        Ok(logs)
    }

    pub async fn update_topic_summary(
        &self,
        topic_id: &str,
        new_summary: &str,
        new_last_id: i64,
    ) -> Result<()> {
        let topic_id = topic_id.to_owned();
        let new_summary = new_summary.to_owned();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE topics SET topic_summary = ?1, last_summarized_log_id = ?2 WHERE id = ?3",
                    rusqlite::params![new_summary, new_last_id, topic_id],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    fn init_schema(conn: &mut Connection) -> rusqlite::Result<()> {
        let tx = conn.transaction()?;

        tx.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS channels (
                id TEXT PRIMARY KEY,
                platform TEXT,
                channel_summary TEXT
            );

            CREATE TABLE IF NOT EXISTS topics (
                id TEXT PRIMARY KEY,
                channel_id TEXT,
                topic_summary TEXT,
                last_summarized_log_id INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS audit_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                channel_id TEXT,
                topic_id TEXT,
                role TEXT,
                content TEXT,
                artifact_paths TEXT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            ",
        )?;

        // Note: FTS5 requires the FTS5 extension to be enabled in SQLite.
        // It is usually enabled by default in rusqlite with the 'bundled' feature.
        tx.execute_batch(
            "
            CREATE VIRTUAL TABLE IF NOT EXISTS fts_audit_logs USING fts5(
                content,
                content='audit_logs',
                content_rowid='id'
            );

            -- Triggers to keep FTS table in sync with audit_logs
            CREATE TRIGGER IF NOT EXISTS audit_logs_ai AFTER INSERT ON audit_logs BEGIN
                INSERT INTO fts_audit_logs(rowid, content) VALUES (new.id, new.content);
            END;

            CREATE TRIGGER IF NOT EXISTS audit_logs_ad AFTER DELETE ON audit_logs BEGIN
                INSERT INTO fts_audit_logs(fts_audit_logs, rowid, content) VALUES ('delete', old.id, old.content);
            END;

            CREATE TRIGGER IF NOT EXISTS audit_logs_au AFTER UPDATE ON audit_logs BEGIN
                INSERT INTO fts_audit_logs(fts_audit_logs, rowid, content) VALUES ('delete', old.id, old.content);
                INSERT INTO fts_audit_logs(rowid, content) VALUES (new.id, new.content);
            END;
            "
        )?;

        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_schema_initialization() -> Result<()> {
        let db = Db::open_in_memory().await?;

        db.conn
            .call(|conn| {
                // Verify tables exist
                let mut stmt = conn
                    .prepare("SELECT name FROM sqlite_master WHERE type='table' OR type='view'")?;
                let mut rows = stmt.query([])?;

                let mut tables = Vec::new();
                while let Some(row) = rows.next()? {
                    let name: String = row.get(0)?;
                    tables.push(name);
                }

                assert!(tables.contains(&"channels".to_string()));
                assert!(tables.contains(&"topics".to_string()));
                assert!(tables.contains(&"audit_logs".to_string()));
                assert!(tables.contains(&"fts_audit_logs".to_string()));

                Ok(())
            })
            .await?;

        Ok(())
    }
}
