use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::exec;
use crate::scheduler::{self, Scheduler};
use crate::spec::{hash_spec, Instance, Spec};
use crate::state::StateDB;

pub fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", now.as_secs())
}

pub fn reconcile(
    db: &StateDB,
    specs: &HashMap<String, Spec>,
    nodes: &[String],
    scheduler: &Scheduler,
    exec_create_template: &str,
    exec_delete_template: &str,
    exec_health_template: &str,
) -> Vec<String> {
    let mut actions: Vec<String> = Vec::new();

    if let Err(e) = db.register_nodes(nodes) {
        actions.push(format!("db error registering nodes: {}", e));
    }

    collect_garbage(db, specs, exec_delete_template, &mut actions);

    for spec in specs.values() {
        let current_hash = hash_spec(spec);

        let instances = db.get_instances(&spec.name).unwrap_or_else(|e| {
            actions.push(format!("db error loading instances for '{}': {}", spec.name, e));
            vec![]
        });

        let running: Vec<&Instance> = instances
            .iter()
            .filter(|i| i.status == "running" || i.status == "desired")
            .collect();

        let healthy: Vec<&Instance> = running
            .iter()
            .filter(|i| check_health(i, exec_health_template))
            .copied()
            .collect();

        let old_hash_instances: Vec<&Instance> = instances
            .iter()
            .filter(|i| i.spec_hash != current_hash && !i.spec_hash.is_empty())
            .filter(|i| i.status != "deleting")
            .collect();

        if !old_hash_instances.is_empty() {
            actions.push(format!(
                "spec '{}' changed, {} instances with old hash need rolling update",
                spec.name,
                old_hash_instances.len()
            ));

            let mut next_idx = instances.len() as u32;

            for _ in 0..spec.replicas {
                match schedule_instance(db, spec, &current_hash, nodes, scheduler, next_idx) {
                    Ok(inst) => {
                        let cmd = exec::substitute(exec_create_template, &inst.id, &inst.node);
                        actions.push(format!("create: {} on {} -> {}", inst.id, inst.node, cmd));

                        match exec::run_command(&cmd) {
                            Ok(true) => {
                                let mut new_inst = inst;
                                new_inst.status = "running".into();
                                if let Err(e) = db.insert_instance(&new_inst) {
                                    actions.push(format!("db error inserting {}: {}", new_inst.id, e));
                                } else {
                                    actions.push(format!("created: {}", new_inst.id));
                                }
                            }
                            Ok(false) => {
                                actions.push(format!("create failed: {}", inst.id));
                            }
                            Err(e) => {
                                actions.push(format!("create error: {}", e));
                            }
                        }
                        next_idx += 1;
                    }
                    Err(e) => {
                        actions.push(format!("schedule error: {}", e));
                        break;
                    }
                }
            }

            for old in &old_hash_instances {
                let cmd = exec::substitute(exec_delete_template, &old.id, &old.node);
                actions.push(format!("drain/delete old: {} -> {}", old.id, cmd));

                if let Err(e) = db.update_instance_status(&old.id, "draining") {
                    actions.push(format!("db error marking {} draining: {}", old.id, e));
                }
                if let Err(e) = exec::run_command(&cmd) {
                    actions.push(format!("delete command error for {}: {}", old.id, e));
                }
                if let Err(e) = db.delete_instance(&old.id) {
                    actions.push(format!("db error deleting {}: {}", old.id, e));
                }
            }

            continue;
        }

        let current_count = healthy.len() as u32;
        let desired = spec.replicas;

        if current_count < desired {
            let needed = desired - current_count;
            actions.push(format!(
                "spec '{}': {} healthy, need {} more",
                spec.name, current_count, needed
            ));

            let mut next_idx = instances.len() as u32;

            for _i in 0..needed {
                match schedule_instance(db, spec, &current_hash, nodes, scheduler, next_idx) {
                    Ok(inst) => {
                        let cmd =
                            exec::substitute(exec_create_template, &inst.id, &inst.node);
                        actions.push(format!(
                            "create: {} on {} -> {}",
                            inst.id, inst.node, cmd
                        ));

                        match exec::run_command(&cmd) {
                            Ok(true) => {
                                let mut new_inst = inst;
                                new_inst.status = "running".into();
                                if let Err(e) = db.insert_instance(&new_inst) {
                                    actions.push(format!("db error inserting {}: {}", new_inst.id, e));
                                } else {
                                    actions.push(format!("created: {}", new_inst.id));
                                }
                            }
                            Ok(false) => {
                                actions.push(format!("create failed: {}", inst.id));
                            }
                            Err(e) => {
                                actions.push(format!("create error: {}", e));
                            }
                        }
                        next_idx += 1;
                    }
                    Err(e) => {
                        actions.push(format!("schedule error: {}", e));
                        break;
                    }
                }
            }
        } else if current_count > desired {
            let excess = current_count - desired;
            actions.push(format!(
                "spec '{}': {} running, {} excess",
                spec.name, current_count, excess
            ));

            let mut to_delete: Vec<&Instance> = running
                .iter()
                .filter(|i| !healthy.iter().any(|h| h.id == i.id))
                .copied()
                .collect();

            for inst in &running {
                if to_delete.len() >= excess as usize {
                    break;
                }
                if !to_delete.contains(inst) {
                    to_delete.push(inst);
                }
            }

            to_delete.truncate(excess as usize);

            for inst in &to_delete {
                let cmd = exec::substitute(exec_delete_template, &inst.id, &inst.node);
                actions.push(format!("delete: {} on {} -> {}", inst.id, inst.node, cmd));

                if let Err(e) = db.update_instance_status(&inst.id, "deleting") {
                    actions.push(format!("db error marking {} deleting: {}", inst.id, e));
                }
                if let Err(e) = exec::run_command(&cmd) {
                    actions.push(format!("delete command error for {}: {}", inst.id, e));
                }
                if let Err(e) = db.delete_instance(&inst.id) {
                    actions.push(format!("db error deleting {}: {}", inst.id, e));
                }
            }
        } else {
            actions.push(format!(
                "spec '{}': {} replicas healthy (no change)",
                spec.name, current_count
            ));
        }
    }

    actions
}

fn collect_garbage(
    db: &StateDB,
    specs: &HashMap<String, Spec>,
    exec_delete_template: &str,
    actions: &mut Vec<String>,
) {
    let all_instances = match db.get_all_instances() {
        Ok(insts) => insts,
        Err(e) => {
            actions.push(format!("db error loading all instances for gc: {}", e));
            return;
        }
    };

    let mut orphaned_specs: HashMap<String, Vec<Instance>> = HashMap::new();
    for inst in &all_instances {
        if !specs.contains_key(&inst.spec_name) {
            orphaned_specs
                .entry(inst.spec_name.clone())
                .or_default()
                .push(inst.clone());
        }
    }

    for (spec_name, instances) in &orphaned_specs {
        actions.push(format!(
            "gc: spec '{}' removed, cleaning up {} orphaned instances",
            spec_name,
            instances.len()
        ));

        for inst in instances {
            let cmd = exec::substitute(exec_delete_template, &inst.id, &inst.node);
            actions.push(format!("gc: delete orphan {} -> {}", inst.id, cmd));

            if let Err(e) = exec::run_command(&cmd) {
                actions.push(format!("gc: delete error for {}: {}", inst.id, e));
            }
            if let Err(e) = db.delete_instance(&inst.id) {
                actions.push(format!("gc: db error deleting {}: {}", inst.id, e));
            }
        }
    }
}

fn schedule_instance(
    db: &StateDB,
    spec: &Spec,
    current_hash: &str,
    nodes: &[String],
    scheduler: &Scheduler,
    next_index: u32,
) -> Result<Instance, String> {
    let up_nodes = db.get_up_nodes().unwrap_or_default();
    let available: Vec<String> = if up_nodes.is_empty() {
        nodes.to_vec()
    } else {
        up_nodes
    };

    let node_loads: Vec<(String, u32)> = available
        .iter()
        .map(|n| {
            let count = db.instance_count_on_node(n).unwrap_or(0);
            (n.clone(), count)
        })
        .collect();

    let result = scheduler::schedule(scheduler, &available, &node_loads, &spec.name, next_index)
        .ok_or_else(|| "no available nodes".to_string())?;

    Ok(Instance {
        id: result.instance_id,
        spec_name: spec.name.clone(),
        node: result.node,
        status: "desired".into(),
        spec_hash: current_hash.into(),
        created_at: timestamp(),
    })
}

fn check_health(inst: &Instance, exec_health_template: &str) -> bool {
    if exec_health_template.is_empty() {
        return inst.status == "running";
    }

    let cmd = exec::substitute(exec_health_template, &inst.id, &inst.node);
    exec::run_command(&cmd).unwrap_or(false)
}

pub fn check_node_health(
    db: &StateDB,
    nodes: &[String],
    node_health_cmd: &str,
) -> Vec<String> {
    let mut actions = Vec::new();

    for node in nodes {
        if node_health_cmd.is_empty() {
            continue;
        }

        let cmd = exec::substitute(node_health_cmd, "", node);
        let healthy = exec::run_command(&cmd).unwrap_or(false);

        if healthy {
            if let Err(e) = db.mark_node_up(node) {
                actions.push(format!("db error marking node {} up: {}", node, e));
            }
        } else {
            actions.push(format!("node {} is down", node));
            if let Err(e) = db.mark_node_down(node) {
                actions.push(format!("db error marking node {} down: {}", node, e));
            }
        }
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::parse_spec;
    use std::path::Path;

    fn test_db() -> StateDB {
        StateDB::open(Path::new(":memory:")).unwrap()
    }

    fn test_nodes() -> Vec<String> {
        vec!["node1".into(), "node2".into()]
    }

    #[test]
    fn reconcile_creates_when_below_desired() {
        let db = test_db();
        db.register_nodes(&test_nodes()).unwrap();

        let spec = parse_spec("name: test\nreplicas: 2\n").unwrap();
        let mut specs = HashMap::new();
        specs.insert("test".into(), spec);

        let actions = reconcile(
            &db,
            &specs,
            &test_nodes(),
            &Scheduler::FirstFit,
            "echo create {name} on {node}",
            "echo delete {name}",
            "",
        );

        assert!(actions.iter().any(|a| a.contains("need 2 more")));
        assert!(actions.iter().any(|a| a.contains("created:")));
    }

    #[test]
    fn reconcile_deletes_when_above_desired() {
        let db = test_db();
        db.register_nodes(&test_nodes()).unwrap();

        let spec = parse_spec("name: test\nreplicas: 1\n").unwrap();
        let hash = hash_spec(&spec);

        db.insert_instance(&Instance {
            id: "inst-1".into(),
            spec_name: "test".into(),
            node: "node1".into(),
            status: "running".into(),
            spec_hash: hash.clone(),
            created_at: "0".into(),
        })
        .unwrap();

        db.insert_instance(&Instance {
            id: "inst-2".into(),
            spec_name: "test".into(),
            node: "node2".into(),
            status: "running".into(),
            spec_hash: hash.clone(),
            created_at: "0".into(),
        })
        .unwrap();

        let mut specs = HashMap::new();
        specs.insert("test".into(), spec);

        let actions = reconcile(
            &db,
            &specs,
            &test_nodes(),
            &Scheduler::FirstFit,
            "echo create {name}",
            "echo delete {name}",
            "",
        );

        assert!(actions.iter().any(|a| a.contains("excess")));
        assert!(actions.iter().any(|a| a.contains("delete:")));
    }

    #[test]
    fn reconcile_no_change_when_at_desired() {
        let db = test_db();
        db.register_nodes(&test_nodes()).unwrap();

        let spec = parse_spec("name: test\nreplicas: 1\n").unwrap();
        let hash = hash_spec(&spec);

        db.insert_instance(&Instance {
            id: "inst-1".into(),
            spec_name: "test".into(),
            node: "node1".into(),
            status: "running".into(),
            spec_hash: hash.clone(),
            created_at: "0".into(),
        })
        .unwrap();

        let mut specs = HashMap::new();
        specs.insert("test".into(), spec);

        let actions = reconcile(
            &db,
            &specs,
            &test_nodes(),
            &Scheduler::FirstFit,
            "echo create {name}",
            "echo delete {name}",
            "",
        );

        assert!(actions.iter().any(|a| a.contains("no change")));
    }

    #[test]
    fn garbage_collects_orphaned_instances() {
        let db = test_db();
        db.register_nodes(&test_nodes()).unwrap();

        db.insert_instance(&Instance {
            id: "orphan-1".into(),
            spec_name: "deleted-spec".into(),
            node: "node1".into(),
            status: "running".into(),
            spec_hash: "abc".into(),
            created_at: "0".into(),
        })
        .unwrap();

        let specs: HashMap<String, Spec> = HashMap::new();
        let actions = reconcile(
            &db,
            &specs,
            &test_nodes(),
            &Scheduler::FirstFit,
            "echo create {name}",
            "echo delete {name}",
            "",
        );

        assert!(actions.iter().any(|a| a.contains("orphaned")));
        assert!(actions.iter().any(|a| a.contains("orphan-1")));
    }
}
