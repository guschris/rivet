pub enum Scheduler {
    FirstFit,
    BestFit,
}

impl Scheduler {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "first-fit" => Ok(Scheduler::FirstFit),
            "best-fit" => Ok(Scheduler::BestFit),
            other => Err(format!("unknown scheduler '{}', expected 'first-fit' or 'best-fit'", other)),
        }
    }
}

pub struct ScheduleResult {
    pub node: String,
    pub instance_id: String,
}

pub fn schedule(
    strategy: &Scheduler,
    nodes: &[String],
    node_loads: &[(String, u32)],
    spec_name: &str,
    next_index: u32,
) -> Option<ScheduleResult> {
    if nodes.is_empty() {
        return None;
    }

    let load_map: std::collections::HashMap<&str, u32> = node_loads
        .iter()
        .map(|(n, c)| (n.as_str(), *c))
        .collect();

    let sorted: Vec<&String> = match strategy {
        Scheduler::FirstFit => first_fit_order(nodes, &load_map),
        Scheduler::BestFit => best_fit_order(nodes, &load_map),
    };

    if let Some(node) = sorted.first() {
        let instance_id = format!("{}-{:08x}", spec_name, next_index);
        return Some(ScheduleResult {
            node: (*node).clone(),
            instance_id,
        });
    }

    None
}

fn first_fit_order<'a>(
    nodes: &'a [String],
    load_map: &std::collections::HashMap<&str, u32>,
) -> Vec<&'a String> {
    let mut indexed: Vec<(usize, u32, &String)> = nodes
        .iter()
        .enumerate()
        .map(|(idx, node)| (idx, load_map.get(node.as_str()).copied().unwrap_or(0), node))
        .collect();
    indexed.sort_by_key(|(idx, load, _)| (*load, *idx));
    indexed.into_iter().map(|(_, _, node)| node).collect()
}

fn best_fit_order<'a>(
    nodes: &'a [String],
    load_map: &std::collections::HashMap<&str, u32>,
) -> Vec<&'a String> {
    let mut indexed: Vec<(u32, &String)> = nodes
        .iter()
        .map(|node| (load_map.get(node.as_str()).copied().unwrap_or(0), node))
        .collect();
    indexed.sort_by_key(|(load, _)| *load);
    indexed.into_iter().map(|(_, node)| node).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nodes() -> Vec<String> {
        vec!["node1".into(), "node2".into(), "node3".into()]
    }

    #[test]
    fn first_fit_picks_least_loaded() {
        let loads = vec![
            ("node1".into(), 5u32),
            ("node2".into(), 2u32),
            ("node3".into(), 4u32),
        ];

        let result = schedule(&Scheduler::FirstFit, &nodes(), &loads, "test", 0).unwrap();
        assert_eq!(result.node, "node2");
    }

    #[test]
    fn best_fit_picks_least_loaded() {
        let loads = vec![
            ("node1".into(), 5u32),
            ("node2".into(), 2u32),
            ("node3".into(), 4u32),
        ];

        let result = schedule(&Scheduler::BestFit, &nodes(), &loads, "test", 0).unwrap();
        assert_eq!(result.node, "node2");
    }

    #[test]
    fn returns_none_for_empty_nodes() {
        let loads = vec![];
        let result = schedule(&Scheduler::FirstFit, &[], &loads, "test", 0);
        assert!(result.is_none());
    }

    #[test]
    fn first_fit_breaks_ties_by_index() {
        let loads = vec![
            ("node1".into(), 3u32),
            ("node2".into(), 3u32),
            ("node3".into(), 3u32),
        ];

        let result = schedule(&Scheduler::FirstFit, &nodes(), &loads, "test", 0).unwrap();
        assert_eq!(result.node, "node1");
    }
}
