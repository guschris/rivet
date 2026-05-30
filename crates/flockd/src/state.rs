use rusqlite::{params, Connection};
use std::path::Path;

use crate::spec::Rollout;
pub use crate::spec::Instance;

pub struct StateDB {
    conn: Connection,
}

#[allow(dead_code)]
impl StateDB {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path)
            .map_err(|e| format!("cannot open state db: {}", e))?;

        conn.busy_timeout(std::time::Duration::from_secs(30))
            .map_err(|e| format!("cannot set busy timeout: {}", e))?;

        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| format!("cannot set WAL mode: {}", e))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS instances (
                id TEXT PRIMARY KEY,
                spec_name TEXT NOT NULL,
                node TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'desired',
                spec_hash TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS nodes (
                name TEXT PRIMARY KEY,
                status TEXT NOT NULL DEFAULT 'up'
            );
            CREATE TABLE IF NOT EXISTS rollouts (
                spec_name TEXT PRIMARY KEY,
                new_hash TEXT NOT NULL,
                phase TEXT NOT NULL DEFAULT 'creating',
                created_count INTEGER NOT NULL DEFAULT 0
            );"
        )
        .map_err(|e| format!("cannot create tables: {}", e))?;

        Ok(StateDB { conn })
    }

    pub fn load_nodes(&self) -> Result<Vec<String>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM nodes")
            .map_err(|e| format!("query error: {}", e))?;

        let nodes: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("query error: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(nodes)
    }

    pub fn register_nodes(&self, nodes: &[String]) -> Result<(), String> {
        for node in nodes {
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO nodes (name, status) VALUES (?1, 'up')",
                    params![node],
                )
                .map_err(|e| format!("insert node error: {}", e))?;
        }
        Ok(())
    }

    pub fn mark_node_down(&self, node: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE nodes SET status = 'down' WHERE name = ?1",
                params![node],
            )
            .map_err(|e| format!("update node error: {}", e))?;
        Ok(())
    }

    pub fn mark_node_up(&self, node: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE nodes SET status = 'up' WHERE name = ?1",
                params![node],
            )
            .map_err(|e| format!("update node error: {}", e))?;
        Ok(())
    }

    pub fn get_node_status(&self, node: &str) -> Result<String, String> {
        self.conn
            .query_row(
                "SELECT status FROM nodes WHERE name = ?1",
                params![node],
                |row| row.get(0),
            )
            .map_err(|e| format!("node query error: {}", e))
    }

    pub fn get_up_nodes(&self) -> Result<Vec<String>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM nodes WHERE status = 'up'")
            .map_err(|e| format!("query error: {}", e))?;

        let nodes: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("query error: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(nodes)
    }

    pub fn insert_instance(&self, inst: &Instance) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO instances (id, spec_name, node, status, spec_hash, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    inst.id,
                    inst.spec_name,
                    inst.node,
                    inst.status,
                    inst.spec_hash,
                    inst.created_at,
                ],
            )
            .map_err(|e| format!("insert instance error: {}", e))?;
        Ok(())
    }

    pub fn update_instance_status(&self, id: &str, status: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE instances SET status = ?1 WHERE id = ?2",
                params![status, id],
            )
            .map_err(|e| format!("update instance error: {}", e))?;
        Ok(())
    }

    pub fn update_instance_hash(&self, id: &str, spec_hash: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE instances SET spec_hash = ?1 WHERE id = ?2",
                params![spec_hash, id],
            )
            .map_err(|e| format!("update instance error: {}", e))?;
        Ok(())
    }

    pub fn delete_instance(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM instances WHERE id = ?1", params![id])
            .map_err(|e| format!("delete instance error: {}", e))?;
        Ok(())
    }

    pub fn get_instances(&self, spec_name: &str) -> Result<Vec<Instance>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, spec_name, node, status, spec_hash, created_at
                 FROM instances WHERE spec_name = ?1",
            )
            .map_err(|e| format!("query error: {}", e))?;

        let instances: Vec<Instance> = stmt
            .query_map(params![spec_name], |row| {
                Ok(Instance {
                    id: row.get(0)?,
                    spec_name: row.get(1)?,
                    node: row.get(2)?,
                    status: row.get(3)?,
                    spec_hash: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| format!("query error: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(instances)
    }

    pub fn get_all_instances(&self) -> Result<Vec<Instance>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, spec_name, node, status, spec_hash, created_at FROM instances",
            )
            .map_err(|e| format!("query error: {}", e))?;

        let instances: Vec<Instance> = stmt
            .query_map([], |row| {
                Ok(Instance {
                    id: row.get(0)?,
                    spec_name: row.get(1)?,
                    node: row.get(2)?,
                    status: row.get(3)?,
                    spec_hash: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| format!("query error: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(instances)
    }

    pub fn instance_count_on_node(&self, node: &str) -> Result<u32, String> {
        let count: u32 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM instances WHERE node = ?1 AND status != 'deleting'",
                params![node],
                |row| row.get(0),
            )
            .map_err(|e| format!("count error: {}", e))?;
        Ok(count)
    }

    pub fn get_rollout(&self, spec_name: &str) -> Result<Option<Rollout>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT spec_name, new_hash, phase, created_count FROM rollouts WHERE spec_name = ?1",
            )
            .map_err(|e| format!("query error: {}", e))?;

        let mut rows = stmt
            .query_map(params![spec_name], |row| {
                Ok(Rollout {
                    spec_name: row.get(0)?,
                    new_hash: row.get(1)?,
                    phase: row.get(2)?,
                    created_count: row.get(3)?,
                })
            })
            .map_err(|e| format!("query error: {}", e))?;

        match rows.next() {
            Some(Ok(rollout)) => Ok(Some(rollout)),
            Some(Err(e)) => Err(format!("rollout query error: {}", e)),
            None => Ok(None),
        }
    }

    pub fn insert_rollout(&self, rollout: &Rollout) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO rollouts (spec_name, new_hash, phase, created_count)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    rollout.spec_name,
                    rollout.new_hash,
                    rollout.phase,
                    rollout.created_count,
                ],
            )
            .map_err(|e| format!("insert rollout error: {}", e))?;
        Ok(())
    }

    pub fn update_rollout_phase(&self, spec_name: &str, phase: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE rollouts SET phase = ?1 WHERE spec_name = ?2",
                params![phase, spec_name],
            )
            .map_err(|e| format!("update rollout error: {}", e))?;
        Ok(())
    }

    pub fn increment_rollout_count(&self, spec_name: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE rollouts SET created_count = created_count + 1 WHERE spec_name = ?1",
                params![spec_name],
            )
            .map_err(|e| format!("update rollout count error: {}", e))?;
        Ok(())
    }

    pub fn delete_rollout(&self, spec_name: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM rollouts WHERE spec_name = ?1", params![spec_name])
            .map_err(|e| format!("delete rollout error: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> StateDB {
        StateDB::open(Path::new(":memory:")).unwrap()
    }

    fn test_instance(name: &str, node: &str, status: &str) -> Instance {
        Instance {
            id: name.into(),
            spec_name: "test-spec".into(),
            node: node.into(),
            status: status.into(),
            spec_hash: "abc123".into(),
            created_at: "2024-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn insert_and_retrieve_instance() {
        let db = test_db();
        let inst = test_instance("inst-1", "node1", "running");
        db.insert_instance(&inst).unwrap();

        let instances = db.get_instances("test-spec").unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id, "inst-1");
    }

    #[test]
    fn update_instance_status() {
        let db = test_db();
        let inst = test_instance("inst-2", "node1", "desired");
        db.insert_instance(&inst).unwrap();
        db.update_instance_status("inst-2", "running").unwrap();

        let instances = db.get_instances("test-spec").unwrap();
        assert_eq!(instances[0].status, "running");
    }

    #[test]
    fn delete_instance() {
        let db = test_db();
        let inst = test_instance("inst-3", "node1", "running");
        db.insert_instance(&inst).unwrap();
        db.delete_instance("inst-3").unwrap();

        let instances = db.get_instances("test-spec").unwrap();
        assert!(instances.is_empty());
    }

    #[test]
    fn node_management() {
        let db = test_db();
        db.register_nodes(&["node1".into(), "node2".into()])
            .unwrap();

        let nodes = db.get_up_nodes().unwrap();
        assert_eq!(nodes.len(), 2);

        db.mark_node_down("node1").unwrap();
        let up = db.get_up_nodes().unwrap();
        assert_eq!(up.len(), 1);
        assert_eq!(up[0], "node2");

        db.mark_node_up("node1").unwrap();
        let up = db.get_up_nodes().unwrap();
        assert_eq!(up.len(), 2);
    }

    #[test]
    fn instance_count_per_node() {
        let db = test_db();
        db.insert_instance(&test_instance("a", "node1", "running"))
            .unwrap();
        db.insert_instance(&test_instance("b", "node1", "desired"))
            .unwrap();
        db.insert_instance(&test_instance("c", "node2", "running"))
            .unwrap();

        assert_eq!(db.instance_count_on_node("node1").unwrap(), 2);
        assert_eq!(db.instance_count_on_node("node2").unwrap(), 1);
    }

    #[test]
    fn rollout_lifecycle() {
        let db = test_db();
        let rollout = Rollout::new("test-spec", "abc123");
        db.insert_rollout(&rollout).unwrap();

        let fetched = db.get_rollout("test-spec").unwrap().unwrap();
        assert_eq!(fetched.phase, "creating");
        assert_eq!(fetched.created_count, 0);

        db.increment_rollout_count("test-spec").unwrap();
        let fetched = db.get_rollout("test-spec").unwrap().unwrap();
        assert_eq!(fetched.created_count, 1);

        db.update_rollout_phase("test-spec", "waiting_healthy").unwrap();
        let fetched = db.get_rollout("test-spec").unwrap().unwrap();
        assert_eq!(fetched.phase, "waiting_healthy");

        db.delete_rollout("test-spec").unwrap();
        assert!(db.get_rollout("test-spec").unwrap().is_none());
    }
}
