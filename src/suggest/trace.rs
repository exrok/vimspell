use std::cell::RefCell;
use std::fmt;

use crate::MAXWLEN_EXT;

use super::State;

pub enum Action {
    EnterState(State),
    GoDeeper { child: u32 },
    Suggest { word: Vec<u8>, score: i32 },
}

pub struct TryStateTrace {
    pub id: u32,
    pub query: [u8; 24],
    pub prefix: [u8; 24],
    pub depth: u8,
    pub score: i32,
    pub actions: Vec<Action>,
}

pub struct Trace {
    pub nodes: Vec<TryStateTrace>,
    pub current: [u32; MAXWLEN_EXT],
}

impl std::ops::Index<u8> for Trace {
    type Output = TryStateTrace;

    fn index(&self, index: u8) -> &Self::Output {
        let id = self.current[index as usize];
        &self.nodes[id as usize]
    }
}

impl std::ops::IndexMut<u8> for Trace {
    fn index_mut(&mut self, index: u8) -> &mut Self::Output {
        let id = self.current[index as usize];
        &mut self.nodes[id as usize]
    }
}

thread_local! {
    static TRACER: RefCell<Option<Trace>> = const { RefCell::new(None) };
}

pub fn enable_trace() {
    TRACER.with_borrow_mut(|t| {
        *t = Some(Trace::new());
    });
}

pub fn take_trace() -> Option<Trace> {
    TRACER.with_borrow_mut(|t| t.take())
}

#[allow(dead_code)]
pub fn with_trace(f: impl FnOnce(&mut Trace)) {
    TRACER.with_borrow_mut(|t| {
        if let Some(trace) = t.as_mut() {
            f(trace);
        }
    });
}

impl Trace {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            current: [0; MAXWLEN_EXT],
        }
    }

    pub fn init(&mut self, depth: u8, query: &[u8], prefix: &[u8], score: i32) {
        let id = self.nodes.len() as u32;
        self.nodes.push(TryStateTrace {
            id,
            query: truncated_24(query),
            prefix: truncated_24(prefix),
            depth: depth,
            score,
            actions: Vec::new(),
        });
        self.current[depth as usize] = id;
    }

    pub fn go_deeper(&mut self, parent_depth: u8, query: &[u8], prefix: &[u8], child_score: i32) {
        let child_id = self.nodes.len() as u32;

        let parent_id = self.current[parent_depth as usize];
        self.nodes[parent_id as usize]
            .actions
            .push(Action::GoDeeper { child: child_id });

        let child_depth = parent_depth + 1;
        self.nodes.push(TryStateTrace {
            id: child_id,
            query: truncated_24(query),
            prefix: truncated_24(prefix),
            depth: child_depth,
            score: child_score,
            actions: Vec::new(),
        });
        self.current[child_depth as usize] = child_id;
    }

    pub fn enter_state(&mut self, depth: u8, state: State) {
        let node_id = self.current[depth as usize];
        self.nodes[node_id as usize]
            .actions
            .push(Action::EnterState(state));
    }

    pub fn suggest(&mut self, depth: u8, word: &[u8], score: i32) {
        let node_id = self.current[depth as usize];
        self.nodes[node_id as usize].actions.push(Action::Suggest {
            word: word.to_vec(),
            score,
        });
    }
}

fn truncated_24(bytes: &[u8]) -> [u8; 24] {
    let mut prefix = [0u8; 24];
    let len = bytes.len().min(24);
    prefix[..len].copy_from_slice(&bytes[..len]);
    prefix
}

const STATE_NAMES: [&str; 17] = [
    "Start",
    "Plain",
    "InsPrep",
    "Ins",
    "Swap",
    "Unswap",
    "Swap3",
    "Unswap3",
    "UnRot3l",
    "UnRot3r",
    "RepIni",
    "Rep",
    "RepUndo",
    "RepsalIni",
    "Repsal",
    "RepsalUndo",
    "Final",
];

impl fmt::Display for Trace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut state_counts = [0u64; 17];
        let mut go_deeper_total = 0u64;
        let mut suggest_map: std::collections::HashMap<Vec<u8>, Vec<i32>> =
            std::collections::HashMap::new();
        let mut depth_node_counts = [0u32; MAXWLEN_EXT];
        let mut max_depth: u8 = 0;

        // Per-depth: [state_idx] -> count
        let mut depth_state_counts = [[0u64; 17]; MAXWLEN_EXT];
        // Per-depth: branching factors (go_deeper count per node)
        let mut depth_branching: [Vec<u32>; MAXWLEN_EXT] = std::array::from_fn(|_| Vec::new());
        // Per-depth: leaf count (nodes with 0 GoDeeper actions)
        let mut depth_leaf_counts = [0u32; MAXWLEN_EXT];
        // Which state was active when GoDeeper happened
        let mut go_deeper_from_state = [0u64; 17];
        // Actions per node (for hot node analysis)
        let mut actions_per_node: Vec<(u32, u32)> = Vec::with_capacity(self.nodes.len());

        for node in &self.nodes {
            let d = node.depth as usize;
            depth_node_counts[d] += 1;
            if node.depth > max_depth {
                max_depth = node.depth;
            }

            let mut node_go_deeper = 0u32;
            let mut last_state: Option<u8> = None;

            for action in &node.actions {
                match action {
                    Action::EnterState(s) => {
                        let si = *s as usize;
                        state_counts[si] += 1;
                        depth_state_counts[d][si] += 1;
                        last_state = Some(*s as u8);
                    }
                    Action::GoDeeper { .. } => {
                        go_deeper_total += 1;
                        node_go_deeper += 1;
                        if let Some(s) = last_state {
                            go_deeper_from_state[s as usize] += 1;
                        }
                    }
                    Action::Suggest { word, score } => {
                        suggest_map.entry(word.clone()).or_default().push(*score);
                    }
                }
            }

            actions_per_node.push((node.actions.len() as u32, node.id));
            depth_branching[d].push(node_go_deeper);
            if node_go_deeper == 0 {
                depth_leaf_counts[d] += 1;
            }
        }

        let total_state_entries: u64 = state_counts.iter().sum();
        let total_suggestions: usize = suggest_map.values().map(|v| v.len()).sum();

        // === Header ===
        writeln!(f, "Nodes:      {:>10}", self.nodes.len())?;
        writeln!(f, "Max depth:  {:>10}", max_depth)?;
        writeln!(f, "Go deeper:  {:>10}", go_deeper_total)?;

        // === Global state distribution ===
        if total_state_entries > 0 {
            writeln!(f, "\nState entries ({total_state_entries}):")?;
            let mut indexed: Vec<(usize, u64)> = state_counts
                .iter()
                .enumerate()
                .filter(|(_, c)| **c > 0)
                .map(|(i, &c)| (i, c))
                .collect();
            indexed.sort_by(|a, b| b.1.cmp(&a.1));
            for &(idx, count) in &indexed {
                let pct = count as f64 / total_state_entries as f64 * 100.0;
                writeln!(f, "  {:<15} {:>10} ({:5.1}%)", STATE_NAMES[idx], count, pct)?;
            }
        }

        // === GoDeeper by triggering state ===
        writeln!(f, "\nGoDeeper by triggering state:")?;
        let mut gd_indexed: Vec<(usize, u64)> = go_deeper_from_state
            .iter()
            .enumerate()
            .filter(|(_, c)| **c > 0)
            .map(|(i, &c)| (i, c))
            .collect();
        gd_indexed.sort_by(|a, b| b.1.cmp(&a.1));
        for &(idx, count) in &gd_indexed {
            let pct = count as f64 / go_deeper_total as f64 * 100.0;
            writeln!(f, "  {:<15} {:>10} ({:5.1}%)", STATE_NAMES[idx], count, pct)?;
        }

        // === Nodes per depth with branching & leaf stats ===
        writeln!(
            f,
            "\n{:<10} {:>10} {:>10} {:>10} {:>10} {:>8}",
            "Depth", "Nodes", "Leaves", "AvgBranch", "MaxBranch", "Leaf%"
        )?;
        for d in 0..=max_depth as usize {
            let n = depth_node_counts[d];
            if n == 0 {
                continue;
            }
            let leaves = depth_leaf_counts[d];
            let br = &depth_branching[d];
            let max_br = br.iter().max().copied().unwrap_or(0);
            let sum_br: u64 = br.iter().map(|&x| x as u64).sum();
            let avg_br = sum_br as f64 / n as f64;
            let leaf_pct = leaves as f64 / n as f64 * 100.0;
            writeln!(
                f,
                "  {:>3}       {:>10} {:>10} {:>10.1} {:>10} {:>7.1}%",
                d, n, leaves, avg_br, max_br, leaf_pct
            )?;
        }

        // === State amplification: entries per node ===
        // Shows which states are "looping" states (entered many times per node)
        // vs "one-shot" states (entered ~once per node).
        writeln!(f, "\nState entries per node (amplification):")?;
        let node_count = self.nodes.len() as f64;
        {
            let mut indexed: Vec<(usize, u64)> = state_counts
                .iter()
                .enumerate()
                .filter(|(_, c)| **c > 0)
                .map(|(i, &c)| (i, c))
                .collect();
            indexed.sort_by(|a, b| b.1.cmp(&a.1));
            for &(idx, count) in &indexed {
                let ratio = count as f64 / node_count;
                writeln!(f, "  {:<15} {:>10.1}x per node", STATE_NAMES[idx], ratio)?;
            }
        }

        // === Per-depth amplification: state entries / nodes ===
        writeln!(
            f,
            "\n{:<10} {:>10} {:>12} {:>10}   {:<}",
            "Depth", "Nodes", "StateEntries", "Ratio", "Top amplifiers"
        )?;
        for d in 0..=max_depth as usize {
            let n = depth_node_counts[d] as u64;
            if n == 0 {
                continue;
            }
            let row = &depth_state_counts[d];
            let total: u64 = row.iter().sum();
            let ratio = total as f64 / n as f64;

            // Find top 2 amplifying states at this depth (highest per-node ratio)
            let mut pairs: Vec<(usize, f64)> = row
                .iter()
                .enumerate()
                .filter(|(_, c)| **c > 0)
                .map(|(i, &c)| (i, c as f64 / n as f64))
                .collect();
            pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let top: Vec<String> = pairs
                .iter()
                .take(2)
                .map(|&(i, r)| format!("{}:{:.1}x", STATE_NAMES[i], r))
                .collect();
            writeln!(
                f,
                "  {:>3}       {:>10} {:>12} {:>10.1}   {}",
                d,
                n,
                total,
                ratio,
                top.join(", ")
            )?;
        }

        // === Per-depth state breakdown (top 3 states at each depth) ===
        writeln!(f, "\nPer-depth state breakdown (top 3):")?;
        for d in 0..=max_depth as usize {
            let row = &depth_state_counts[d];
            let total: u64 = row.iter().sum();
            if total == 0 {
                continue;
            }
            let mut pairs: Vec<(usize, u64)> = row
                .iter()
                .enumerate()
                .filter(|(_, c)| **c > 0)
                .map(|(i, &c)| (i, c))
                .collect();
            pairs.sort_by(|a, b| b.1.cmp(&a.1));
            let top3: Vec<String> = pairs
                .iter()
                .take(3)
                .map(|&(i, c)| {
                    format!(
                        "{}:{} ({:.0}%)",
                        STATE_NAMES[i],
                        c,
                        c as f64 / total as f64 * 100.0
                    )
                })
                .collect();
            writeln!(f, "  depth {:>3}: {}", d, top3.join(", "))?;
        }

        // === Actions per node distribution ===
        if !actions_per_node.is_empty() {
            actions_per_node.sort_by(|a, b| a.0.cmp(&b.0));
            let n = actions_per_node.len();
            let p50 = actions_per_node[n / 2].0;
            let p90 = actions_per_node[n * 90 / 100].0;
            let p99 = actions_per_node[n * 99 / 100].0;
            let max_entry = actions_per_node.last().unwrap();
            let min_entry = actions_per_node.first().unwrap();
            writeln!(f, "\nActions per node:")?;
            writeln!(
                f,
                "  min: {}  p50: {}  p90: {}  p99: {}  max: {} (node #{})",
                min_entry.0, p50, p90, p99, max_entry.0, max_entry.1
            )?;

            // Top 10 hottest nodes
            writeln!(f, "\nTop 10 hottest nodes:")?;
            actions_per_node.sort_by(|a, b| b.0.cmp(&a.0));
            for &(count, id) in actions_per_node.iter().take(10) {
                let node = &self.nodes[id as usize];
                let query = &node.query;
                let query_end = query.iter().position(|&b| b == 0).unwrap_or(24);
                let query_str = str::from_utf8(&query[..query_end]).unwrap();
                let prefix = &node.prefix;
                let prefix_end = prefix.iter().position(|&b| b == 0).unwrap_or(24);
                let prefix_str = str::from_utf8(&prefix[..prefix_end]).unwrap();
                // Count action types in this node
                let mut n_states = 0u32;
                let mut n_gd = 0u32;
                let mut n_sug = 0u32;
                for a in &node.actions {
                    match a {
                        Action::EnterState(_) => n_states += 1,
                        Action::GoDeeper { .. } => n_gd += 1,
                        Action::Suggest { .. } => n_sug += 1,
                    }
                }
                writeln!(
                    f,
                    "  node {:>6} depth {:>2} query={:<16} prefix={:<16} actions={:<6} (states:{} deeper:{} suggest:{})",
                    id, node.depth, query_str, prefix_str, count, n_states, n_gd, n_sug
                )?;
            }
        }

        // === Suggestions ===
        writeln!(
            f,
            "\nSuggestions: {} unique, {} total paths",
            suggest_map.len(),
            total_suggestions
        )?;
        let mut suggestions: Vec<(Vec<u8>, Vec<i32>)> = suggest_map.into_iter().collect();
        suggestions.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
        for (word, scores) in suggestions.iter().take(25) {
            let word_str = String::from_utf8_lossy(word);
            let min = scores.iter().min().unwrap();
            let max = scores.iter().max().unwrap();
            if scores.len() > 1 {
                writeln!(
                    f,
                    "  {:>4}x {:<20} (scores: {}..{})",
                    scores.len(),
                    word_str,
                    min,
                    max
                )?;
            } else {
                writeln!(
                    f,
                    "  {:>4}x {:<20} (score: {})",
                    scores.len(),
                    word_str,
                    min
                )?;
            }
        }
        if suggestions.len() > 25 {
            writeln!(f, "  ... and {} more", suggestions.len() - 25)?;
        }

        // === Score distribution (cumulative) ===
        // How many nodes would be explored at each maxscore threshold?
        let thresholds = [50, 75, 100, 125, 150, 175, 200, 250, 300, 350];
        writeln!(f, "\nNodes by score threshold (cumulative):")?;
        writeln!(
            f,
            "  {:<12} {:>10} {:>10} {:>8}",
            "MaxScore", "Nodes", "GoDeeper", "% of total"
        )?;
        for &thresh in &thresholds {
            let mut node_count_t = 0u64;
            let mut gd_count_t = 0u64;
            for node in &self.nodes {
                if node.score < thresh {
                    node_count_t += 1;
                    for action in &node.actions {
                        if matches!(action, Action::GoDeeper { .. }) {
                            gd_count_t += 1;
                        }
                    }
                }
            }
            let pct = node_count_t as f64 / self.nodes.len() as f64 * 100.0;
            writeln!(
                f,
                "  {:<12} {:>10} {:>10} {:>7.1}%",
                thresh, node_count_t, gd_count_t, pct
            )?;
        }

        // === Score histogram (bucketed) ===
        writeln!(f, "\nScore histogram (node creation score):")?;
        let bucket_size = 25;
        let max_score_seen = self.nodes.iter().map(|n| n.score).max().unwrap_or(0);
        let num_buckets = (max_score_seen / bucket_size + 1) as usize;
        let mut buckets = vec![0u64; num_buckets.max(1)];
        for node in &self.nodes {
            let b = (node.score / bucket_size) as usize;
            if b < buckets.len() {
                buckets[b] += 1;
            }
        }
        let max_bucket = *buckets.iter().max().unwrap_or(&1);
        for (i, &count) in buckets.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let lo = i as i32 * bucket_size;
            let hi = lo + bucket_size - 1;
            let bar_len = (count as f64 / max_bucket as f64 * 40.0) as usize;
            let bar: String = "#".repeat(bar_len);
            writeln!(f, "  {:>3}-{:<3} {:>10} {}", lo, hi, count, bar)?;
        }

        // === Suggestion discovery timing ===
        // At what point in the search (node index) is each suggestion first found?
        writeln!(f, "\nSuggestion discovery order:")?;
        let mut first_found: std::collections::HashMap<Vec<u8>, (u32, i32)> =
            std::collections::HashMap::new();
        for node in &self.nodes {
            for action in &node.actions {
                if let Action::Suggest { word, score } = action {
                    first_found.entry(word.clone()).or_insert((node.id, *score));
                }
            }
        }
        let mut discovery: Vec<(u32, Vec<u8>, i32)> = first_found
            .into_iter()
            .map(|(word, (id, score))| (id, word, score))
            .collect();
        discovery.sort_by_key(|&(id, _, _)| id);
        let total_nodes = self.nodes.len() as f64;
        for &(id, ref word, score) in &discovery {
            let word_str = String::from_utf8_lossy(word);
            let pct = id as f64 / total_nodes * 100.0;
            writeln!(
                f,
                "  node {:>7} ({:>5.1}%) score={:<4} {}",
                id, pct, score, word_str
            )?;
        }

        // === Potential maxscore reduction over time ===
        // If we aggressively reduced maxscore after each suggestion,
        // what would maxscore look like over the search?
        writeln!(f, "\nMaxscore reduction potential:")?;
        let mut best_score_so_far = 350i32;
        let mut checkpoints = Vec::new();
        for node in &self.nodes {
            for action in &node.actions {
                if let Action::Suggest { score, .. } = action {
                    if *score < best_score_so_far {
                        best_score_so_far = *score;
                        checkpoints.push((node.id, best_score_so_far));
                    }
                }
            }
        }
        writeln!(
            f,
            "  {:>10} {:>10} {:>12}",
            "At node", "BestScore", "Margin+150"
        )?;
        for &(id, best) in &checkpoints {
            let pct = id as f64 / total_nodes * 100.0;
            writeln!(
                f,
                "  {:>10} {:>10} {:>12}   ({:.1}% through)",
                id,
                best,
                best + 150,
                pct
            )?;
        }

        // === Score vs depth heatmap ===
        writeln!(f, "\nScore vs depth (node counts):")?;
        let score_buckets = [0, 50, 100, 150, 200, 250, 300, 350];
        write!(f, "  {:>5}", "Depth")?;
        for i in 0..score_buckets.len() - 1 {
            write!(
                f,
                " {:>4}-{:<3}",
                score_buckets[i],
                score_buckets[i + 1] - 1
            )?;
        }
        writeln!(f)?;
        for d in 0..=max_depth as usize {
            if depth_node_counts[d] == 0 {
                continue;
            }
            write!(f, "  {:>5}", d)?;
            for i in 0..score_buckets.len() - 1 {
                let lo = score_buckets[i];
                let hi = score_buckets[i + 1];
                let count: u64 = self
                    .nodes
                    .iter()
                    .filter(|n| n.depth as usize == d && n.score >= lo && n.score < hi)
                    .count() as u64;
                if count > 0 {
                    write!(f, " {:>8}", count)?;
                } else {
                    write!(f, " {:>8}", ".")?;
                }
            }
            writeln!(f)?;
        }

        Ok(())
    }
}
