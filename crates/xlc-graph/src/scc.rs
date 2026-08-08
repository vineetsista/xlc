//! Tarjan strongly-connected components + condensation topological order
//! (§8.3). Iterative (explicit stack) — corpus dependency chains can be
//! hundreds of thousands of cells deep and must never overflow the thread
//! stack.
//!
//! Contract: edges point from a formula cell to the cells it *depends on*.
//! Tarjan emits components in reverse topological order of the
//! condensation — every edge goes from a later-emitted component to an
//! earlier-emitted one — which, with depends-on edges, is exactly
//! evaluation order: dependencies first. `schedule` is therefore the
//! emission order unchanged.

/// Compact adjacency list: `edges[node] = nodes it depends on`.
pub type Adj = Vec<Vec<u32>>;

/// Strongly-connected components, each a list of node ids. Singleton
/// components without a self-loop are acyclic cells; everything else is a
/// genuine circular block (iterative-calc node or reported exclusion).
pub fn tarjan_scc(adj: &Adj) -> Vec<Vec<u32>> {
    let n = adj.len();
    const UNVISITED: u32 = u32::MAX;
    let mut index = vec![UNVISITED; n];
    let mut lowlink = vec![0u32; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<u32> = Vec::new();
    let mut next_index = 0u32;
    let mut components: Vec<Vec<u32>> = Vec::new();

    // Explicit DFS frame: (node, edge cursor).
    let mut frames: Vec<(u32, usize)> = Vec::new();

    for start in 0..n as u32 {
        if index[start as usize] != UNVISITED {
            continue;
        }
        frames.push((start, 0));
        while let Some(&mut (v, ref mut cursor)) = frames.last_mut() {
            let vi = v as usize;
            if *cursor == 0 {
                index[vi] = next_index;
                lowlink[vi] = next_index;
                next_index += 1;
                stack.push(v);
                on_stack[vi] = true;
            }
            if let Some(&w) = adj[vi].get(*cursor) {
                *cursor += 1;
                let wi = w as usize;
                if index[wi] == UNVISITED {
                    frames.push((w, 0));
                } else if on_stack[wi] {
                    lowlink[vi] = lowlink[vi].min(index[wi]);
                }
            } else {
                // v is finished.
                frames.pop();
                if let Some(&(parent, _)) = frames.last() {
                    let pi = parent as usize;
                    lowlink[pi] = lowlink[pi].min(lowlink[vi]);
                }
                if lowlink[vi] == index[vi] {
                    let mut comp = Vec::new();
                    loop {
                        let w = stack.pop().expect("tarjan stack underflow");
                        on_stack[w as usize] = false;
                        comp.push(w);
                        if w == v {
                            break;
                        }
                    }
                    components.push(comp);
                }
            }
        }
    }
    components
}

/// Does this component require iterative calculation? True for any
/// multi-node component, or a singleton that references itself.
pub fn is_cyclic(comp: &[u32], adj: &Adj) -> bool {
    match comp {
        [single] => adj[*single as usize].contains(single),
        _ => true,
    }
}

/// Evaluation schedule: components in dependency-first topological order.
/// With depends-on edges this is Tarjan's emission order as-is.
pub fn schedule(adj: &Adj) -> Vec<Vec<u32>> {
    tarjan_scc(adj)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp_of(comps: &[Vec<u32>], node: u32) -> usize {
        comps.iter().position(|c| c.contains(&node)).unwrap()
    }

    #[test]
    fn scc_linear_chain() {
        // 0 -> 1 -> 2 (0 depends on 1 depends on 2)
        let adj: Adj = vec![vec![1], vec![2], vec![]];
        let comps = schedule(&adj);
        assert_eq!(comps.len(), 3);
        // Dependency-first: 2 before 1 before 0.
        assert!(comp_of(&comps, 2) < comp_of(&comps, 1));
        assert!(comp_of(&comps, 1) < comp_of(&comps, 0));
        assert!(comps.iter().all(|c| !is_cyclic(c, &adj)));
    }

    #[test]
    fn scc_simple_cycle() {
        // 0 <-> 1, plus 2 -> 0. One 2-cycle + one singleton.
        let adj: Adj = vec![vec![1], vec![0], vec![0]];
        let comps = schedule(&adj);
        assert_eq!(comps.len(), 2);
        let cycle = comps.iter().find(|c| c.len() == 2).unwrap();
        assert!(is_cyclic(cycle, &adj));
        // The cycle must be scheduled before its dependent.
        assert!(comp_of(&comps, 0) < comp_of(&comps, 2));
    }

    #[test]
    fn scc_self_loop_is_cyclic() {
        // A1 = A1+1 style: singleton with a self-edge.
        let adj: Adj = vec![vec![0], vec![]];
        let comps = tarjan_scc(&adj);
        let own = comps.iter().find(|c| c.contains(&0)).unwrap();
        assert_eq!(own.len(), 1);
        assert!(is_cyclic(own, &adj));
        assert!(!is_cyclic(&[1], &adj));
    }

    #[test]
    fn scc_two_tangled_cycles() {
        // {0,1,2} cycle, {3,4} cycle, 2 -> 3 (big cycle depends on small).
        let adj: Adj = vec![vec![1], vec![2], vec![0, 3], vec![4], vec![3]];
        let comps = schedule(&adj);
        assert_eq!(comps.len(), 2);
        assert!(comp_of(&comps, 3) < comp_of(&comps, 0));
        assert!(comps.iter().all(|c| is_cyclic(c, &adj)));
    }

    #[test]
    fn scc_deep_chain_no_stack_overflow() {
        // 300k-deep chain: must not blow the thread stack.
        let n = 300_000u32;
        let mut adj: Adj = (0..n).map(|i| if i + 1 < n { vec![i + 1] } else { vec![] }).collect();
        adj[0].push(1); // duplicate edge for good measure
        let comps = schedule(&adj);
        assert_eq!(comps.len(), n as usize);
        assert_eq!(comps.first().unwrap(), &vec![n - 1]);
        assert_eq!(comps.last().unwrap(), &vec![0]);
    }

    #[test]
    fn scc_diamond() {
        // 0 -> {1,2} -> 3
        let adj: Adj = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let comps = schedule(&adj);
        assert_eq!(comps.len(), 4);
        assert!(comp_of(&comps, 3) < comp_of(&comps, 1));
        assert!(comp_of(&comps, 3) < comp_of(&comps, 2));
        assert!(comp_of(&comps, 1) < comp_of(&comps, 0));
        assert!(comp_of(&comps, 2) < comp_of(&comps, 0));
    }
}
