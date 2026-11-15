//! Listener ordering: `after`/`before` constraints first, then higher
//! priority, then registration order, so the order is stable across
//! runs. A constraint naming a listener that is not there is reported and
//! ignored; a cycle is an error naming the listeners in it.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

/// A listener as ordering sees it.
#[derive(Clone, Debug)]
pub(crate) struct Node<'a> {
    pub id: &'a str,
    pub after: &'a [String],
    pub before: &'a [String],
    pub priority: i32,
    /// Registration sequence: the last tie-breaker.
    pub seq: u64,
}

/// The resolved order, as indexes into the nodes, and every constraint
/// naming a missing listener, as `(listener, missing target)`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Resolved {
    pub order: Vec<usize>,
    pub missing: Vec<(String, String)>,
}

/// Orders the nodes, or returns a cycle as the ids in it, the first
/// repeated at the end (`a -> b -> a`: `a` must run before `b`, which
/// must run before `a`).
pub(crate) fn resolve(nodes: &[Node<'_>]) -> Result<Resolved, Vec<String>> {
    let index: BTreeMap<&str, usize> = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let mut missing = Vec::new();
    // `edges[i]` lists the nodes that must run after `i`.
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    let mut waiting = vec![0usize; nodes.len()];
    let mut edge = |from: usize, to: usize, edges: &mut Vec<Vec<usize>>| {
        if !edges[from].contains(&to) {
            edges[from].push(to);
            waiting[to] += 1;
        }
    };
    for (i, n) in nodes.iter().enumerate() {
        for a in n.after {
            match index.get(a.as_str()) {
                Some(&j) => edge(j, i, &mut edges),
                None => missing.push((n.id.to_owned(), a.clone())),
            }
        }
        for b in n.before {
            match index.get(b.as_str()) {
                Some(&j) => edge(i, j, &mut edges),
                None => missing.push((n.id.to_owned(), b.clone())),
            }
        }
    }
    // Ready listeners: highest priority first, then the earliest
    // registered.
    let key = |i: usize| (nodes[i].priority, Reverse(nodes[i].seq), i);
    let mut ready: BinaryHeap<_> = (0..nodes.len())
        .filter(|&i| waiting[i] == 0)
        .map(key)
        .collect();
    let mut order = Vec::with_capacity(nodes.len());
    while let Some((_, _, i)) = ready.pop() {
        order.push(i);
        for &d in &edges[i] {
            waiting[d] -= 1;
            if waiting[d] == 0 {
                ready.push(key(d));
            }
        }
    }
    if order.len() == nodes.len() {
        return Ok(Resolved { order, missing });
    }
    // A cycle: from a node left, follow edges among the nodes left until
    // one repeats.
    let mut at = (0..nodes.len())
        .find(|&i| waiting[i] > 0)
        .expect("a node left");
    let mut path: Vec<usize> = Vec::new();
    loop {
        if let Some(pos) = path.iter().position(|&p| p == at) {
            let mut cycle: Vec<String> = path[pos..]
                .iter()
                .map(|&i| nodes[i].id.to_owned())
                .collect();
            cycle.push(nodes[at].id.to_owned());
            return Err(cycle);
        }
        path.push(at);
        at = edges[at]
            .iter()
            .copied()
            .find(|&j| waiting[j] > 0)
            .expect("a node in a cycle has a successor left");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ids in order and the missing targets.
    type Ran = (Vec<&'static str>, Vec<(String, String)>);

    struct Spec {
        id: &'static str,
        after: Vec<String>,
        before: Vec<String>,
        priority: i32,
    }

    fn s(id: &'static str, after: &[&str], before: &[&str], priority: i32) -> Spec {
        Spec {
            id,
            after: after.iter().map(|s| s.to_string()).collect(),
            before: before.iter().map(|s| s.to_string()).collect(),
            priority,
        }
    }

    fn run(specs: &[Spec]) -> Result<Ran, Vec<String>> {
        let nodes: Vec<Node<'_>> = specs
            .iter()
            .enumerate()
            .map(|(i, s)| Node {
                id: s.id,
                after: &s.after,
                before: &s.before,
                priority: s.priority,
                seq: i as u64,
            })
            .collect();
        resolve(&nodes).map(|r| (r.order.iter().map(|&i| specs[i].id).collect(), r.missing))
    }

    #[test]
    fn constraints_then_priority_then_registration() {
        let (order, missing) = run(&[
            s("a", &[], &[], 0),
            s("b", &[], &[], 10),
            s("c", &["a"], &[], 100),
            s("d", &[], &["a"], -5),
            s("e", &[], &[], 0),
        ])
        .unwrap();
        // Among the listeners ready, higher priority first: b, then e
        // (a waits for d); d (-5) then frees a, which frees c.
        assert_eq!(order, ["b", "e", "d", "a", "c"]);
        assert!(missing.is_empty());
    }

    #[test]
    fn missing_targets_are_reported_and_ignored() {
        let (order, missing) =
            run(&[s("a", &["ghost"], &[], 0), s("b", &[], &["spook"], 1)]).unwrap();
        assert_eq!(order, ["b", "a"]);
        assert_eq!(
            missing,
            [
                ("a".to_string(), "ghost".to_string()),
                ("b".to_string(), "spook".to_string())
            ]
        );
    }

    #[test]
    fn cycles_name_their_listeners() {
        let cycle = run(&[
            s("x", &[], &[], 0),
            s("a", &["c"], &[], 0),
            s("b", &["a"], &[], 0),
            s("c", &["b"], &[], 0),
        ])
        .unwrap_err();
        assert_eq!(cycle, ["a", "b", "c", "a"]);
        let mixed = run(&[s("a", &[], &["b"], 0), s("b", &[], &["a"], 0)]).unwrap_err();
        assert_eq!(mixed.len(), 3);
        assert_eq!(run(&[s("s", &["s"], &[], 0)]).unwrap_err(), ["s", "s"]);
    }
}
