use std::{collections::HashMap, path::Path};

use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};

use crate::{
    error::{Error, Result},
    models::{
        ActivityEvent, AppSettings, Backup, BackupSchedule, HangarPluginMetadata, JavaRuntime,
        LogSession, ManagedDatabase, ModMetadata, Server, ServerStatus,
    },
};

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        let db = Self { pool };
        db.migrate().await?;
        Ok(db)
    }

    async fn migrate(&self) -> Result<()> {
        for statement in [
            "CREATE TABLE IF NOT EXISTS servers (id TEXT PRIMARY KEY, folder TEXT NOT NULL UNIQUE, data TEXT NOT NULL, updated_at INTEGER NOT NULL)",
            "CREATE TABLE IF NOT EXISTS app_settings (id INTEGER PRIMARY KEY CHECK(id = 1), data TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS java_runtimes (id TEXT PRIMARY KEY, path TEXT NOT NULL UNIQUE, data TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS backups (id TEXT PRIMARY KEY, server_id TEXT NOT NULL, created_at INTEGER NOT NULL, data TEXT NOT NULL)",
            "CREATE INDEX IF NOT EXISTS backups_server_created ON backups(server_id, created_at DESC)",
            "CREATE TABLE IF NOT EXISTS backup_schedules (server_id TEXT PRIMARY KEY, data TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS activity (id TEXT PRIMARY KEY, at INTEGER NOT NULL, data TEXT NOT NULL)",
            "CREATE INDEX IF NOT EXISTS activity_at ON activity(at DESC)",
            "CREATE TABLE IF NOT EXISTS log_sessions (id TEXT PRIMARY KEY, server_id TEXT NOT NULL, started_at INTEGER NOT NULL, data TEXT NOT NULL)",
            "CREATE INDEX IF NOT EXISTS sessions_server_started ON log_sessions(server_id, started_at DESC)",
            "CREATE TABLE IF NOT EXISTS plugin_metadata (server_id TEXT NOT NULL, project_id INTEGER NOT NULL, file_name TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY(server_id, project_id), UNIQUE(server_id, file_name), FOREIGN KEY(server_id) REFERENCES servers(id) ON DELETE CASCADE)",
            "CREATE INDEX IF NOT EXISTS plugin_metadata_server ON plugin_metadata(server_id)",
            "CREATE TABLE IF NOT EXISTS mod_metadata (server_id TEXT NOT NULL, provider TEXT NOT NULL, project_id TEXT NOT NULL, file_name TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY(server_id, provider, project_id), UNIQUE(server_id, file_name), FOREIGN KEY(server_id) REFERENCES servers(id) ON DELETE CASCADE)",
            "CREATE INDEX IF NOT EXISTS mod_metadata_server ON mod_metadata(server_id)",
            "CREATE TABLE IF NOT EXISTS managed_databases (id TEXT PRIMARY KEY, server_id TEXT NOT NULL, created_at INTEGER NOT NULL, data TEXT NOT NULL, FOREIGN KEY(server_id) REFERENCES servers(id) ON DELETE RESTRICT)",
            "CREATE INDEX IF NOT EXISTS managed_databases_server ON managed_databases(server_id, created_at DESC)",
            "PRAGMA user_version = 4",
        ] {
            sqlx::query(statement).execute(&self.pool).await?;
        }
        Ok(())
    }

    pub async fn load_servers(&self) -> Result<Vec<Server>> {
        let rows = sqlx::query("SELECT data FROM servers ORDER BY updated_at ASC")
            .fetch_all(&self.pool)
            .await?;
        let mut servers = Vec::with_capacity(rows.len());
        for row in rows {
            let mut server: Server = serde_json::from_str(row.get::<&str, _>("data"))?;
            server.status = ServerStatus::Stopped;
            server.players = 0;
            server.started_at = None;
            server.memory = 0.0;
            server.cpu = 0.0;
            server.active_operation = None;
            server.history.clear();
            server.alerts.retain(|alert| alert.kind != "stop-timeout");
            servers.push(server);
        }
        Ok(servers)
    }

    pub async fn save_server(&self, server: &Server) -> Result<()> {
        let data = serde_json::to_string(server)?;
        sqlx::query(
            "INSERT INTO servers(id, folder, data, updated_at) VALUES(?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET folder=excluded.folder, data=excluded.data, updated_at=excluded.updated_at",
        )
        .bind(&server.id)
        .bind(&server.folder)
        .bind(data)
        .bind(crate::models::now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_server(&self, id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM backup_schedules WHERE server_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM log_sessions WHERE server_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM servers WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn load_databases(&self, server_id: &str) -> Result<Vec<ManagedDatabase>> {
        let rows = sqlx::query(
            "SELECT data FROM managed_databases WHERE server_id = ? ORDER BY created_at DESC",
        )
        .bind(server_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| serde_json::from_str(row.get::<&str, _>("data")).map_err(Error::from))
            .collect()
    }

    pub async fn load_database(&self, id: &str) -> Result<Option<ManagedDatabase>> {
        let row = sqlx::query("SELECT data FROM managed_databases WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| serde_json::from_str(row.get::<&str, _>("data")).map_err(Error::from))
            .transpose()
    }

    pub async fn save_database(&self, database: &ManagedDatabase) -> Result<()> {
        let data = serde_json::to_string(database)?;
        sqlx::query(
            "INSERT INTO managed_databases(id, server_id, created_at, data) VALUES(?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET data=excluded.data",
        )
        .bind(&database.id)
        .bind(&database.server_id)
        .bind(database.created_at)
        .bind(data)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_database(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM managed_databases WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn count_databases(&self, server_id: &str) -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM managed_databases WHERE server_id = ?")
                .bind(server_id)
                .fetch_one(&self.pool)
                .await?,
        )
    }

    pub async fn load_plugin_metadata(&self, server_id: &str) -> Result<Vec<HangarPluginMetadata>> {
        let rows = sqlx::query("SELECT data FROM plugin_metadata WHERE server_id = ?")
            .bind(server_id)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| serde_json::from_str(row.get::<&str, _>("data")).map_err(Error::from))
            .collect()
    }

    pub async fn save_plugin_metadata(&self, metadata: &HangarPluginMetadata) -> Result<()> {
        let data = serde_json::to_string(metadata)?;
        sqlx::query(
            "INSERT INTO plugin_metadata(server_id, project_id, file_name, data) VALUES(?, ?, ?, ?) \
             ON CONFLICT(server_id, project_id) DO UPDATE SET file_name=excluded.file_name, data=excluded.data",
        )
        .bind(&metadata.server_id)
        .bind(metadata.project_id as i64)
        .bind(&metadata.file_name)
        .bind(data)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn rename_plugin_metadata_file(
        &self,
        server_id: &str,
        old_file_name: &str,
        new_file_name: &str,
    ) -> Result<()> {
        let Some(row) =
            sqlx::query("SELECT data FROM plugin_metadata WHERE server_id = ? AND file_name = ?")
                .bind(server_id)
                .bind(old_file_name)
                .fetch_optional(&self.pool)
                .await?
        else {
            return Ok(());
        };
        let mut metadata: HangarPluginMetadata = serde_json::from_str(row.get::<&str, _>("data"))?;
        metadata.file_name = new_file_name.to_owned();
        self.save_plugin_metadata(&metadata).await
    }

    pub async fn delete_plugin_metadata_file(
        &self,
        server_id: &str,
        file_name: &str,
    ) -> Result<()> {
        sqlx::query("DELETE FROM plugin_metadata WHERE server_id = ? AND file_name = ?")
            .bind(server_id)
            .bind(file_name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn load_mod_metadata(&self, server_id: &str) -> Result<Vec<ModMetadata>> {
        let rows = sqlx::query("SELECT data FROM mod_metadata WHERE server_id = ?")
            .bind(server_id)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| serde_json::from_str(row.get::<&str, _>("data")).map_err(Error::from))
            .collect()
    }

    pub async fn save_mod_metadata(&self, metadata: &ModMetadata) -> Result<()> {
        let data = serde_json::to_string(metadata)?;
        sqlx::query(
            "INSERT INTO mod_metadata(server_id, provider, project_id, file_name, data) VALUES(?, ?, ?, ?, ?) \
             ON CONFLICT(server_id, provider, project_id) DO UPDATE SET file_name=excluded.file_name, data=excluded.data",
        )
        .bind(&metadata.server_id)
        .bind(&metadata.provider)
        .bind(&metadata.project_id)
        .bind(&metadata.file_name)
        .bind(data)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn rename_mod_metadata_file(
        &self,
        server_id: &str,
        old_file_name: &str,
        new_file_name: &str,
    ) -> Result<()> {
        let Some(row) =
            sqlx::query("SELECT data FROM mod_metadata WHERE server_id = ? AND file_name = ?")
                .bind(server_id)
                .bind(old_file_name)
                .fetch_optional(&self.pool)
                .await?
        else {
            return Ok(());
        };
        let mut metadata: ModMetadata = serde_json::from_str(row.get::<&str, _>("data"))?;
        metadata.file_name = new_file_name.to_owned();
        self.save_mod_metadata(&metadata).await
    }

    pub async fn delete_mod_metadata_file(&self, server_id: &str, file_name: &str) -> Result<()> {
        sqlx::query("DELETE FROM mod_metadata WHERE server_id = ? AND file_name = ?")
            .bind(server_id)
            .bind(file_name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn load_settings(&self, defaults: AppSettings) -> Result<AppSettings> {
        let row = sqlx::query("SELECT data FROM app_settings WHERE id = 1")
            .fetch_optional(&self.pool)
            .await?;
        if let Some(row) = row {
            Ok(serde_json::from_str(row.get::<&str, _>("data"))?)
        } else {
            self.save_settings(&defaults).await?;
            Ok(defaults)
        }
    }

    pub async fn save_settings(&self, settings: &AppSettings) -> Result<()> {
        let data = serde_json::to_string(settings)?;
        sqlx::query(
            "INSERT INTO app_settings(id, data) VALUES(1, ?) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
        )
        .bind(data)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_runtimes(&self) -> Result<Vec<JavaRuntime>> {
        self.load_json_rows("SELECT data FROM java_runtimes ORDER BY id")
            .await
    }

    pub async fn save_runtime(&self, runtime: &JavaRuntime) -> Result<()> {
        let data = serde_json::to_string(runtime)?;
        sqlx::query(
            "INSERT INTO java_runtimes(id, path, data) VALUES(?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET path=excluded.path, data=excluded.data",
        )
        .bind(&runtime.id)
        .bind(&runtime.path)
        .bind(data)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_runtime(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM java_runtimes WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn load_backups(&self) -> Result<Vec<Backup>> {
        self.load_json_rows("SELECT data FROM backups ORDER BY created_at DESC")
            .await
    }

    pub async fn save_backup(&self, backup: &Backup) -> Result<()> {
        let data = serde_json::to_string(backup)?;
        sqlx::query(
            "INSERT INTO backups(id, server_id, created_at, data) VALUES(?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET data=excluded.data",
        )
        .bind(&backup.id)
        .bind(&backup.server_id)
        .bind(backup.created_at)
        .bind(data)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_backup(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM backups WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn load_schedules(&self) -> Result<HashMap<String, BackupSchedule>> {
        let rows = sqlx::query("SELECT server_id, data FROM backup_schedules")
            .fetch_all(&self.pool)
            .await?;
        let mut result = HashMap::new();
        for row in rows {
            let id: String = row.get("server_id");
            let schedule = serde_json::from_str(row.get::<&str, _>("data"))?;
            result.insert(id, schedule);
        }
        Ok(result)
    }

    pub async fn save_schedule(&self, server_id: &str, schedule: &BackupSchedule) -> Result<()> {
        let data = serde_json::to_string(schedule)?;
        sqlx::query(
            "INSERT INTO backup_schedules(server_id, data) VALUES(?, ?) \
             ON CONFLICT(server_id) DO UPDATE SET data=excluded.data",
        )
        .bind(server_id)
        .bind(data)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_activity(&self, limit: i64) -> Result<Vec<ActivityEvent>> {
        let rows = sqlx::query("SELECT data FROM activity ORDER BY at DESC LIMIT ?")
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| serde_json::from_str(row.get::<&str, _>("data")).map_err(Error::from))
            .collect()
    }

    pub async fn save_activity(&self, event: &ActivityEvent) -> Result<()> {
        let data = serde_json::to_string(event)?;
        sqlx::query("INSERT OR REPLACE INTO activity(id, at, data) VALUES(?, ?, ?)")
            .bind(&event.id)
            .bind(event.at)
            .bind(data)
            .execute(&self.pool)
            .await?;
        sqlx::query(
            "DELETE FROM activity WHERE id NOT IN (SELECT id FROM activity ORDER BY at DESC LIMIT 500)",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_sessions(&self) -> Result<Vec<LogSession>> {
        self.load_json_rows("SELECT data FROM log_sessions ORDER BY started_at DESC LIMIT 500")
            .await
    }

    pub async fn save_session(&self, session: &LogSession) -> Result<()> {
        let data = serde_json::to_string(session)?;
        sqlx::query(
            "INSERT INTO log_sessions(id, server_id, started_at, data) VALUES(?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET data=excluded.data",
        )
        .bind(&session.id)
        .bind(&session.server_id)
        .bind(session.started_at)
        .bind(data)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn load_json_rows<T: serde::de::DeserializeOwned>(&self, query: &str) -> Result<Vec<T>> {
        let rows = sqlx::query(query).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|row| serde_json::from_str(row.get::<&str, _>("data")).map_err(Error::from))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn creates_the_versioned_wal_database() {
        let temporary = tempfile::tempdir().unwrap();
        let database = Database::open(&temporary.path().join("nooki.db"))
            .await
            .unwrap();
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&database.pool)
            .await
            .unwrap();
        let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&database.pool)
            .await
            .unwrap();
        let metadata_table: String = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'plugin_metadata'",
        )
        .fetch_one(&database.pool)
        .await
        .unwrap();
        let mod_metadata_table: String = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'mod_metadata'",
        )
        .fetch_one(&database.pool)
        .await
        .unwrap();
        let databases_table: String = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'managed_databases'",
        )
        .fetch_one(&database.pool)
        .await
        .unwrap();
        assert_eq!(version, 4);
        assert_eq!(journal.to_ascii_lowercase(), "wal");
        assert_eq!(metadata_table, "plugin_metadata");
        assert_eq!(mod_metadata_table, "mod_metadata");
        assert_eq!(databases_table, "managed_databases");
    }

    #[tokio::test]
    async fn persists_and_renames_hangar_plugin_metadata() {
        let temporary = tempfile::tempdir().unwrap();
        let database = Database::open(&temporary.path().join("nooki.db"))
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO servers(id, folder, data, updated_at) VALUES('paper', 'server-folder', '{}', 0)",
        )
        .execute(&database.pool)
        .await
        .unwrap();
        let metadata = HangarPluginMetadata {
            server_id: "paper".into(),
            file_name: "Example.jar".into(),
            project_id: 42,
            namespace: "Author".into(),
            slug: "Example".into(),
            name: "Example Plugin".into(),
            description: "Description".into(),
            author: "Author".into(),
            version: "1.2.3".into(),
        };
        database.save_plugin_metadata(&metadata).await.unwrap();
        database
            .rename_plugin_metadata_file("paper", "Example.jar", "Example.jar.disabled")
            .await
            .unwrap();
        let loaded = database.load_plugin_metadata("paper").await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].file_name, "Example.jar.disabled");
        assert_eq!(loaded[0].project_id, 42);
    }

    #[tokio::test]
    async fn persists_and_renames_mod_metadata_from_both_providers() {
        let temporary = tempfile::tempdir().unwrap();
        let database = Database::open(&temporary.path().join("nooki.db"))
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO servers(id, folder, data, updated_at) VALUES('fabric', 'fabric-folder', '{}', 0)",
        )
        .execute(&database.pool)
        .await
        .unwrap();
        for (provider, project_id, file_name) in [
            ("modrinth", "AABBCCDD", "modrinth.jar"),
            ("curseforge", "12345", "curseforge.jar"),
        ] {
            database
                .save_mod_metadata(&ModMetadata {
                    server_id: "fabric".into(),
                    file_name: file_name.into(),
                    provider: provider.into(),
                    project_id: project_id.into(),
                    slug: "example".into(),
                    name: "Example Mod".into(),
                    description: "Description".into(),
                    author: "Author".into(),
                    version: "1.0.0".into(),
                    icon_url: None,
                    website_url: "https://example.invalid".into(),
                })
                .await
                .unwrap();
        }
        database
            .rename_mod_metadata_file("fabric", "modrinth.jar", "modrinth.jar.disabled")
            .await
            .unwrap();
        let loaded = database.load_mod_metadata("fabric").await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(loaded
            .iter()
            .any(|item| item.file_name == "modrinth.jar.disabled"));
        assert!(loaded.iter().any(|item| item.provider == "curseforge"));
    }
}
