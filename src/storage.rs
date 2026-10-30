use crate::state::Snapshot;
use rusqlite::{Connection, params};
use std::path::Path;

pub struct Store {
    connection: Connection,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        let connection = Connection::open(path).map_err(|_| "Cannot open local database")?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS state (id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL); CREATE TABLE IF NOT EXISTS audit (id INTEGER PRIMARY KEY, time INTEGER NOT NULL, event TEXT NOT NULL);").map_err(|_|"Cannot initialize database")?;
        Ok(Self { connection })
    }
    pub fn load(&self) -> Result<Snapshot, String> {
        let result = self
            .connection
            .query_row("SELECT body FROM state WHERE id=1", [], |row| {
                row.get::<_, String>(0)
            });
        match result {
            Ok(body) => {
                let mut s: Snapshot = serde_json::from_str(&body)
                    .map_err(|_| "Stored state is invalid; restore from a backup")?;
                if s.schema != 1 {
                    return Err("Unsupported database schema; refusing to overwrite".into());
                }
                s.running = false;
                s.app_version = env!("CARGO_PKG_VERSION").into();
                s.optimizing = false;
                s.live_enabled = false;
                s.watch.enabled = false;
                s.updating = false;
                Ok(s)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Snapshot::new(),
            Err(_) => Err("Cannot read stored state".into()),
        }
    }
    pub fn save(&mut self, state: &Snapshot, event: &str) -> Result<(), String> {
        let body = serde_json::to_string(state).map_err(|_| "Cannot serialize state")?;
        let tx = self
            .connection
            .transaction()
            .map_err(|_| "Cannot begin database transaction")?;
        tx.execute("INSERT INTO state(id,body) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET body=excluded.body",params![body]).map_err(|_|"Cannot persist state")?;
        tx.execute(
            "INSERT INTO audit(time,event) VALUES(?1,?2)",
            params![chrono::Utc::now().timestamp(), event],
        )
        .map_err(|_| "Cannot persist audit event")?;
        tx.commit().map_err(|_| "Cannot commit state".into())
    }
    pub fn backup(&self, path: &Path) -> Result<(), String> {
        // SQLite creates a consistent snapshot, including committed WAL content.
        self.connection
            .execute("VACUUM INTO ?1", params![path.to_string_lossy().as_ref()])
            .map_err(|_| "Cannot create database backup")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_preserves_positions_and_always_pauses_trading() {
        let dir = tempfile::tempdir().expect("directory");
        let path = dir.path().join("state.sqlite");
        let mut store = Store::open(&path).expect("open");
        let mut state = Snapshot::new().expect("state");
        state.running = true;
        state
            .ledger
            .buy(
                150_000_000,
                150_000_000,
                20,
                123,
                "v1",
                160_000_000,
                140_000_000,
            )
            .expect("buy");
        store.save(&state, "test").expect("save");
        store
            .backup(&dir.path().join("backup.sqlite"))
            .expect("backup");
        drop(store);
        let reloaded = Store::open(&path).expect("open").load().expect("load");
        assert!(!reloaded.running);
        assert_eq!(
            reloaded.ledger.layers[0].quantity,
            state.ledger.layers[0].quantity
        );
        assert!(!reloaded.live_enabled);
    }
}
