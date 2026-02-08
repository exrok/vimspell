use super::*;
use hashbrown::HashMap;

macro_rules! trace {
    ($($tt:tt)*) => {};
}

// Uncomment to enable tracing
// mod trace;
// pub use trace::Trace;
// pub(crate) use trace::{enable_trace, take_trace};
// macro_rules! trace {
//     (init $depth:expr, $query:expr, $prefix:expr, $score:expr) => {
//         trace::with_trace(|t| t.init($depth, $query, $prefix, $score))
//     };
//     (go_deeper $depth:expr, $query:expr, $trace: expr, $child_score:expr) => {
//         trace::with_trace(|t| t.go_deeper($depth, $query, $trace, $child_score))
//     };
//     (enter_state $depth:expr, $state:expr) => {
//         trace::with_trace(|t| t.enter_state($depth, $state))
//     };
//     (suggest $depth:expr, $word:expr, $score:expr) => {
//         trace::with_trace(|t| t.suggest($depth, $word, $score))
//     };
// }

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum State {
    Start,
    Plain,
    InsPrep,
    Ins,
    Swap,
    Unswap,
    Swap3,
    Unswap3,
    UnRot3l,
    UnRot3r,
    RepIni,
    Rep,
    RepUndo,
    RepsalIni,
    Repsal,
    RepsalUndo,
    Final,
}

const TSF_DIDDEL: u8 = 4;

#[derive(Clone, Copy)]
#[repr(C, align(8))]
struct TryState {
    trie_node: u32,
    score: i32,
    /// Current accumulated edit-distance score to reach node.
    /// Points to current trie node's length byte
    state: State,
    flags: u8,
    cursor: i16,
    /// How far have we consumed in the query to get to this position.
    query_pos: u8,
    /// Don't modify characters before this postion
    query_min_pos: u8,
    prefix_len: u8,
    deleted_query_pos: u8,
    /// In the trie-walking states (Start, Plain, InsPrep, Ins) cursor is a child index within a trie node (always >= 1).
    /// But in Rep/Repsal states it's repurposed to hold an index into the rep/repsal arrays,
    /// Tracks last deletion to avoid reinserting at that position
    split_first_word_flags: u32,
    split_prefix_pos: u8,
    split_query_pos: u8,
}

impl Default for TryState {
    fn default() -> Self {
        Self {
            state: State::Start,
            score: 0,
            trie_node: 0,
            cursor: 1,
            query_pos: 0,
            query_min_pos: 0,
            prefix_len: 0,
            flags: 0,
            deleted_query_pos: 0,
            split_prefix_pos: 0,
            split_query_pos: 0,
            split_first_word_flags: 0,
        }
    }
}

const MAX_DEPTH: u8 = 253;
const SUG_CLEAN_COUNT: usize = 150;
const SUG_CLEANUP_HEADROOM: usize = 50;

fn add_suggestion(scored: &mut HashMap<Box<[u8]>, i32>, word: &[u8], score: i32) {
    match scored.entry_ref(word) {
        hashbrown::hash_map::EntryRef::Occupied(mut occupied_entry) => {
            let existing_score = occupied_entry.get_mut();
            if score < *existing_score {
                *existing_score = score;
            }
        }
        hashbrown::hash_map::EntryRef::Vacant(vacant_entry_ref) => {
            vacant_entry_ref.insert(score);
        }
    }
}

/// Reduce maxscore when too many suggestions have accumulated, matching
/// Neovim's cleanup_suggestions() behavior. Returns the new maxscore.
fn cleanup_suggestions(
    scored: &mut HashMap<Box<[u8]>, i32>,
    maxscore: i32,
    clean_count: usize,
) -> i32 {
    let mut scores: Vec<i32> = scored.values().copied().collect();
    if scores.len() <= clean_count {
        return maxscore;
    }
    scores.select_nth_unstable(clean_count - 1);
    let threshold = scores[clean_count - 1];
    scored.retain(|_, &mut score| score <= threshold);
    threshold
}
pub struct Query {
    pub bytes: [u8; MAXWLEN_EXT],
}
impl std::ops::Index<u8> for Query {
    type Output = u8;
    #[inline(always)]
    fn index(&self, index: u8) -> &Self::Output {
        &self.bytes[index as usize]
    }
}

impl std::ops::IndexMut<u8> for Query {
    #[inline(always)]
    fn index_mut(&mut self, index: u8) -> &mut Self::Output {
        &mut self.bytes[index as usize]
    }
}

/// Checks if two characters are similar according to the MAP table.
/// `map_array` is a pointer to the 256-entry map array (or null if no MAP data).
#[inline(always)]
fn similar_chars(map_array: Option<&[u32; 256]>, c1: u8, c2: u8) -> bool {
    if let Some(map) = map_array {
        let m1 = map[c1 as usize];
        m1 != 0 && m1 == map[c2 as usize]
    } else {
        false
    }
    // if map_array.is_null() {
    //     return false;
    // }
    // unsafe {
    //     let m1 = *map_array.add(c1 as usize);
    //     if m1 == 0 {
    //         return false;
    //     }
    //     m1 == *map_array.add(c2 as usize)
    // }
}

/// Check if the suggestion's case is valid given the bad word's case.
/// Matches Neovim's spell_valid_case().
#[inline]
fn spell_valid_case(word_flags: u8, tree_flags: u8) -> bool {
    (word_flags == WF_ALLCAP && (tree_flags & super::WF_FIXCAP) == 0)
        || ((tree_flags & (WF_ALLCAP | WF_KEEPCAP)) == 0
            && ((tree_flags & WF_ONECAP) == 0 || (word_flags & WF_ONECAP) != 0))
}

/// Compute the captype of the result after make_case_word is applied.
#[inline]
fn result_captype(badflags: u8, word_flags: u32) -> u8 {
    if word_flags as u8 & WF_KEEPCAP != 0 {
        return WF_KEEPCAP;
    }
    let combined = badflags as u32 | word_flags;
    if combined & WF_ALLCAP as u32 != 0 {
        WF_ALLCAP
    } else if combined & WF_ONECAP as u32 != 0 {
        WF_ONECAP
    } else {
        0
    }
}

/// Apply case transformation and append the result to `out`.
/// Matches Neovim's make_case_word().
fn apply_case(out: &mut Vec<u8>, word: &[u8], badflags: u8, word_flags: u32) {
    let combined = badflags as u32 | word_flags;
    if combined & WF_ALLCAP as u32 != 0 {
        for &b in word {
            out.push(b.to_ascii_uppercase());
        }
    } else if combined & WF_ONECAP as u32 != 0 {
        if let Some((&first, rest)) = word.split_first() {
            out.push(first.to_ascii_uppercase());
            out.extend_from_slice(rest);
        }
    } else {
        out.extend_from_slice(word);
    }
}

/// Find the keep-case version of a word from the keeptree.
/// Tries both lowercase and uppercase at each position to find the original casing.
fn find_keepcap_word(keeptree: &WordTree, fword: &[u8]) -> Option<Vec<u8>> {
    if keeptree.is_empty() || fword.is_empty() {
        return None;
    }

    let node = &keeptree.node;
    let meta = &keeptree.meta;
    let fword_len = fword.len();

    let uword: Vec<u8> = fword.iter().map(|&b| b.to_ascii_uppercase()).collect();

    // State arrays for iterative DFS (matching Neovim's find_keepcap_word)
    let mut arr = vec![0usize; fword_len + 1];
    let mut rnd = vec![0u8; fword_len + 1];
    let mut kword = vec![0u8; fword_len];

    let mut depth: i32 = 0;
    arr[0] = 0;
    rnd[0] = 0;

    while depth >= 0 {
        let d = depth as usize;

        if d == fword_len {
            // Check for word-end (NUL byte) at this position
            let ai = arr[d];
            if let Some(&len_byte) = node.get(ai) {
                let len = len_byte as usize;
                if len > 0 && node.get(ai + 1) == Some(&0) {
                    return Some(kword.clone());
                }
            }
            depth -= 1;
            continue;
        }

        rnd[d] += 1;

        let c = match rnd[d] {
            1 => fword[d],
            2 => {
                if fword[d] == uword[d] {
                    depth -= 1;
                    continue;
                }
                uword[d]
            }
            _ => {
                depth -= 1;
                continue;
            }
        };

        let ai = arr[d];
        if ai >= node.len() {
            if rnd[d] >= 2 || fword[d] == uword[d] {
                depth -= 1;
            }
            continue;
        }

        let len = node[ai] as usize;
        let base = ai + 1;

        // Skip NUL entries
        let mut nul_count = 0;
        while nul_count < len && base + nul_count < node.len() && node[base + nul_count] == 0 {
            nul_count += 1;
        }

        // Binary search among non-NUL siblings
        let search_start = base + nul_count;
        let search_end = base + len;

        let mut lo = search_start;
        let mut hi = search_end;
        let mut found = false;

        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if mid >= node.len() {
                break;
            }
            if node[mid] == c {
                kword[d] = c;
                arr[d + 1] = meta[mid] as usize;
                rnd[d + 1] = 0;
                depth += 1;
                found = true;
                break;
            } else if node[mid] < c {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }

        if !found && (rnd[d] >= 2 || fword[d] == uword[d]) {
            depth -= 1;
        }
    }

    None
}

/// Build a properly-cased suggestion word, appending to `out`.
fn build_cased_word(
    keeptree: &WordTree,
    word: &[u8],
    badflags: u8,
    word_flags: u32,
    out: &mut Vec<u8>,
) {
    if word_flags as u8 & WF_KEEPCAP != 0
        && let Some(kw) = find_keepcap_word(keeptree, word)
    {
        out.extend_from_slice(&kw);
        return;
    }
    apply_case(out, word, badflags, word_flags);
}

fn rot3_left(query: &mut Query, pos: u8) {
    let a = query[pos];
    let b = query[pos + 1];
    let c = query[pos + 2];
    query[pos] = b;
    query[pos + 1] = c;
    query[pos + 2] = a;
}

fn rot3_right(query: &mut Query, pos: u8) {
    let a = query[pos];
    let b = query[pos + 1];
    let c = query[pos + 2];
    query[pos] = c;
    query[pos + 1] = a;
    query[pos + 2] = b;
}

fn swap2(query: &mut Query, pos: u8) {
    let a = query[pos];
    let b = query[pos + 1];
    query[pos] = b;
    query[pos + 1] = a;
}

fn swap3(query: &mut Query, pos: u8) {
    let a = query[pos];
    let b = query[pos + 2];
    query[pos] = b;
    query[pos + 2] = a;
}

pub(crate) fn suggest_trie_walk(
    dict: &Dictionary,
    query: &mut Query,
    initial_query_len: u8,
    scored: &mut HashMap<Box<[u8]>, i32>,
    maxscore: i32,
    badflags: u8,
    _max_count: usize,
) {
    if dict.foldtree.node.is_empty() {
        return;
    }

    let node: &[u8] = &dict.foldtree.node;
    let meta: &[u32] = &dict.foldtree.meta;

    // Pre-extract the map_array pointer to avoid Option check per iteration.
    let map_array: Option<&[u32; 256]> = dict.map.as_ref().map(|r| &r.map_array);

    let mut max_score = maxscore;
    let mut prefix = [0u8; MAXWLEN_EXT];
    let mut stack = [TryState::default(); MAXWLEN_EXT];
    let mut depth: u8 = 0;
    let mut repextra: i32 = 0;
    let mut query_len = initial_query_len;
    let mut state = State::Start;
    trace!(init 0, &query.bytes, &[], 0);
    let mut current = &mut stack[0];

    macro_rules! recurse {
        ($score_add: expr) => {{
            trace!( go_deeper
                depth,
                &query.bytes[0..query_len as usize],
                &prefix[0..current.prefix_len as usize],
                current.score
            );
            let parent = *current;
            depth += 1;
            current = &mut stack[depth as usize];
            *current = parent;
            current.score += $score_add;
            current.state = State::Start;
            current.flags = 0;
            current.cursor = 1;
            state = State::Start;
        }};
    }
    // When Ins/InsPrep finishes, determine if Swap/Rep/Repsal can all be skipped.
    // SCORE_REP (65) is the cheapest remaining operation; if it's over budget,
    // Swap (75) is too. The query_pos checks match the guards in Swap/RepIni/RepsalIni.
    macro_rules! after_ins {
        () => {
            if current.score + SCORE_REP >= max_score
                || current.query_pos >= query_len
                || current.query_pos < current.query_min_pos
            {
                State::Final
            } else {
                State::Swap
            }
        };
    }
    loop {
        trace!(enter_state depth, state);
        match state {
            State::Start => {
                let node_head = current.trie_node as usize;
                let Some(&len) = node.get(node_head) else {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    current = &mut stack[depth as usize];
                    state = current.state;
                    continue;
                };

                let len = len as i16;
                let cur = current.cursor;

                // perf: Might not need null check
                if cur > len || node[node_head + cur as usize] != 0 {
                    if depth >= MAX_DEPTH {
                        state = State::Final;
                        continue;
                    }
                    if current.query_pos >= query_len {
                        // Inlined Del: query_pos >= eff_fword_len is always true
                        // here, so the go_deeper path is unreachable.
                        current.cursor = 1;
                        state = State::InsPrep;
                        continue;
                    } else {
                        state = State::Plain;
                        continue;
                    }
                }

                current.cursor += 1;

                let flags = meta[node_head + cur as usize];

                if (flags as u16) & WF_NOSUGGEST != 0 {
                    continue;
                }

                let query_pos = current.query_pos;
                let query_ends = query_pos >= query_len;
                let prefix_len = current.prefix_len as usize;

                let goodword_ends = (flags as u16 & WF_NEEDCOMP) == 0;

                if (flags as u8) & WF_BANNED != 0 {
                    continue;
                }

                let mut new_score: i32 = 0;
                if (flags as u8) & WF_REGION != 0 {
                    let region_mask = ((flags >> 16) & 0xff) as u8;
                    if region_mask & dict.region == 0 {
                        new_score += SCORE_REGION;
                    }
                }
                if (flags as u8) & WF_RARE != 0 {
                    new_score += SCORE_RARE;
                }

                // SCORE_ICASE penalty for case mismatch
                let rct = result_captype(badflags, flags);
                if !spell_valid_case(badflags, rct) {
                    new_score += SCORE_ICASE;
                }

                if query_ends && goodword_ends && current.query_pos >= current.query_min_pos {
                    let split_off = current.split_prefix_pos as usize;
                    let is_split = split_off > 0;
                    // Note: moveing this vec out reduces performance.
                    let mut word = Vec::with_capacity(prefix_len + 2);
                    if is_split {
                        build_cased_word(
                            &dict.keeptree,
                            &prefix[..split_off],
                            badflags,
                            current.split_first_word_flags,
                            &mut word,
                        );
                        word.push(b' ');
                    }
                    let second_start = word.len();
                    build_cased_word(
                        &dict.keeptree,
                        &prefix[if is_split { split_off } else { 0 }..prefix_len],
                        badflags,
                        flags,
                        &mut word,
                    );

                    // Apply common word bonus (before SAL, matching Neovim)
                    let total = if !dict.common_words.is_empty() {
                        dict.score_wordcount_adj(
                            current.score + new_score,
                            &word[second_start..],
                            is_split,
                        )
                    } else {
                        current.score + new_score
                    };

                    if total < max_score {
                        add_suggestion(scored, &word, total);
                        trace!(suggest depth, &word, total);

                        if scored.len() > (SUG_CLEAN_COUNT + SUG_CLEANUP_HEADROOM) {
                            max_score = cleanup_suggestions(scored, max_score, SUG_CLEAN_COUNT);
                        }
                    }
                }

                if !query_ends
                    && goodword_ends
                    && current.split_prefix_pos == 0
                    && current.query_pos >= current.query_min_pos
                {
                    let mut extra = new_score + SCORE_SPLIT;
                    // Give a bonus to common first words at the split point
                    if !dict.common_words.is_empty() {
                        extra = dict.score_wordcount_adj(extra, &prefix[..prefix_len], true);
                    }
                    if current.score + extra < max_score {
                        if depth >= MAX_DEPTH {
                            state = State::Final;
                            continue;
                        }
                        let prev_query_pos = current.query_pos;
                        current.state = state;
                        recurse!(extra);

                        current.trie_node = 0;
                        current.split_prefix_pos = prefix_len as u8;
                        current.split_query_pos = prev_query_pos;
                        current.split_first_word_flags = flags;
                        continue;
                    }
                }
            }
            State::Plain => {
                let node_head = current.trie_node as usize;
                let Some(&len) = node.get(node_head) else {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    current = &mut stack[depth as usize];
                    state = current.state;
                    continue;
                };
                let len = len as i16;

                if current.cursor > len {
                    if current.query_pos >= current.query_min_pos {
                        // Inlined Del: query_pos >= fidxtry is always true
                        // here, so only the eff_fword_len guard remains.
                        state = State::InsPrep;
                        current.cursor = 1;

                        let query_pos = current.query_pos;
                        if query_pos < query_len && current.score + SCORE_DEL < max_score {
                            current.state = state;

                            recurse!(SCORE_DEL);

                            current.flags |= TSF_DIDDEL;
                            current.deleted_query_pos = query_pos;
                            current.query_pos += 1;

                            let new_query_pos = current.query_pos;
                            if new_query_pos < query_len && query[new_query_pos] == query[query_pos]
                            {
                                current.score -= SCORE_DEL - SCORE_DELDUP;
                            }
                        }
                    } else {
                        state = State::Final;
                    }
                    continue;
                }

                // When substitution exceeds budget, or deletion was just performed
                // (substitutions after deletion are redundant with the parent's
                // substitution-then-deletion paths), binary search for exact match only.
                if current.score + SCORE_SUBST >= max_score || current.flags & TSF_DIDDEL != 0 {
                    let cursor_start = current.cursor as usize;
                    current.cursor = len + 1;
                    let query_pos = current.query_pos;
                    if query_pos < query_len && current.score < max_score {
                        let target = query[query_pos];
                        let lo = cursor_start;
                        let hi = (len + 1) as usize;
                        let nodes = &node[node_head + lo..node_head + hi];
                        if let Ok(index) = nodes.binary_search(&target) {
                            let idx = node_head + cursor_start + index;
                            current.state = State::Start;
                            recurse!(0);
                            current.query_pos += 1;
                            prefix[current.prefix_len as usize] = target;
                            current.prefix_len += 1;
                            current.trie_node = meta[idx];
                        }
                    }
                    continue;
                }

                let idx = node_head + current.cursor as usize;
                current.cursor += 1;
                let c = node[idx];

                let query_pos = current.query_pos;
                // Match C behavior: always use SCORE_SUBST for pruning.
                // similar_chars adjustment happens post-go_deeper (below).
                let new_score = if query_pos < query_len && c == query[query_pos] {
                    0
                } else if query_pos >= query_len {
                    SCORE_INS
                } else {
                    SCORE_SUBST
                };

                if new_score != 0
                    && (current.query_pos < current.query_min_pos
                        || ((current.flags & TSF_DIDDEL) != 0
                            && c == query[current.deleted_query_pos]))
                {
                    continue;
                }

                if current.score + new_score < max_score {
                    current.state = state;
                    recurse!(new_score);
                    if query_pos < query_len {
                        current.query_pos += 1;
                    }
                    prefix[current.prefix_len as usize] = c;
                    current.prefix_len += 1;
                    current.trie_node = meta[idx];

                    if new_score == SCORE_SUBST
                        && query_pos < query_len
                        && similar_chars(map_array, query[query_pos], c)
                    {
                        current.score -= SCORE_SUBST - SCORE_SIMILAR;
                    }
                }
            }

            State::InsPrep => {
                if current.flags & TSF_DIDDEL != 0
                    || current.score + SCORE_INS >= max_score
                    || (current.query_pos == 0
                        && current.prefix_len >= current.split_prefix_pos + 2)
                {
                    state = after_ins!();
                    continue;
                }

                let arridx = current.trie_node as usize;
                let Some(&len) = node.get(arridx) else {
                    depth -= 1;
                    state = stack[depth as usize].state;
                    current = &mut stack[depth as usize];
                    continue;
                };
                let len = len as i16;

                loop {
                    if current.cursor > len {
                        state = after_ins!();
                        break;
                    }
                    if node[arridx + current.cursor as usize] != 0 {
                        state = State::Ins;
                        break;
                    }
                    current.cursor += 1;
                }
            }

            State::Ins => {
                if current.score + SCORE_INS >= max_score {
                    state = after_ins!();
                    continue;
                }

                let node_head = current.trie_node as usize;
                let Some(&len) = node.get(node_head) else {
                    state = after_ins!();
                    continue;
                };
                let len = len as i16;

                if current.cursor > len {
                    state = after_ins!();
                    continue;
                }

                let node_index = node_head + current.cursor as usize;
                current.cursor += 1;

                let Some(&c) = node.get(node_index) else {
                    state = after_ins!();
                    continue;
                };

                if c == 0 {
                    continue;
                }

                let query_pos = current.query_pos;
                if query_pos < query_len && c == query[query_pos] {
                    continue;
                }

                if current.score + SCORE_INS < max_score {
                    current.state = state;
                    recurse!(SCORE_INS);
                    prefix[current.prefix_len as usize] = c;
                    current.prefix_len += 1;
                    current.trie_node = meta[node_index];

                    let tw_len = current.prefix_len;
                    if tw_len >= 2 && prefix[(tw_len - 2) as usize] == c {
                        current.score -= SCORE_INS - SCORE_INSDUP;
                    }
                }
            }

            State::Swap => {
                let query_pos = current.query_pos;
                if query_pos + 1 >= query_len || current.query_pos < current.query_min_pos {
                    state = State::RepIni;
                    continue;
                }

                let c1 = query[query_pos];
                let c2 = query[query_pos + 1];

                if c1 == c2 {
                    state = State::Swap3;
                    continue;
                }

                if current.score + SCORE_SWAP < max_score {
                    query[query_pos] = c2;
                    query[query_pos + 1] = c1;
                    let prev_query_pos = current.query_pos;
                    state = State::Unswap;
                    current.state = state;
                    recurse!(SCORE_SWAP);
                    current.query_min_pos = prev_query_pos + 2;
                } else {
                    state = State::RepIni;
                }
            }

            State::Unswap => {
                swap2(query, current.query_pos);
                state = State::Swap3;
            }

            State::Swap3 => {
                let query_pos = current.query_pos;
                if query_pos + 2 >= query_len || current.query_pos < current.query_min_pos {
                    state = State::RepIni;
                    continue;
                }

                let c1 = query[query_pos];
                let c3 = query[query_pos + 2];

                if c1 == c3 {
                    state = State::RepIni;
                    continue;
                }

                if current.score + SCORE_SWAP3 < max_score {
                    query[query_pos] = c3;
                    query[query_pos + 2] = c1;
                    state = State::Unswap3;
                    current.state = state;
                    recurse!(SCORE_SWAP3);
                    current.query_min_pos = query_pos + 3;
                } else {
                    state = State::RepIni;
                }
            }

            State::Unswap3 => {
                let query_pos = current.query_pos;
                swap3(query, query_pos);

                if query_pos + 2 < query_len && current.score + SCORE_SWAP3 < max_score {
                    rot3_left(query, query_pos);

                    state = State::UnRot3l;
                    current.state = state;
                    recurse!(SCORE_SWAP3);
                    current.query_min_pos = query_pos + 3;
                    continue;
                }
                state = State::RepIni;
            }

            State::UnRot3l => {
                let query_pos = current.query_pos;
                rot3_right(query, query_pos);
                if current.score + SCORE_SWAP3 < max_score {
                    rot3_right(query, query_pos);
                    state = State::UnRot3r;
                    current.state = state;
                    recurse!(SCORE_SWAP3);
                    current.query_min_pos = query_pos + 3;
                } else {
                    state = State::RepIni;
                }
            }

            State::UnRot3r => {
                rot3_left(query, current.query_pos);
                state = State::RepIni;
            }

            State::RepIni => {
                if dict.rep.is_empty()
                    || current.score + SCORE_REP >= max_score
                    || current.query_pos < current.query_min_pos
                {
                    state = State::RepsalIni;
                    continue;
                }

                let query_pos = current.query_pos;
                if query_pos >= query_len {
                    state = State::RepsalIni;
                    continue;
                }

                let first = dict.rep_first[query[query_pos] as usize];
                if first < 0 {
                    state = State::RepsalIni;
                    continue;
                }

                current.cursor = first;
                state = State::Rep;
            }

            State::Rep => {
                let query_pos = current.query_pos;
                let rep_cursor = current.cursor as usize;

                let Some(&RepItem { from, to }) = dict.rep.get(rep_cursor) else {
                    state = State::RepsalIni;
                    continue;
                };

                // let from_len = from.len();
                // let to_len = to.len();
                let from_bytes = &dict.arena[from];

                if from_bytes[0] != query[query_pos] {
                    state = State::RepsalIni;
                    continue;
                }

                current.cursor += 1;

                if (query_pos as usize) + from.len() > query_len as usize {
                    continue;
                }

                if query.bytes[(query_pos as usize)..(query_pos as usize) + from.len()]
                    != *from_bytes
                {
                    continue;
                }

                if current.score + SCORE_REP >= max_score {
                    continue;
                }

                let to_bytes = &dict.arena[to];

                state = State::RepUndo;
                current.state = state;

                if from.len() != to.len() {
                    let query_pos = query_pos as usize;
                    let tail_start = query_pos + from.len();
                    let tail_end = query_len as usize;
                    if tail_start <= tail_end
                        && query_pos + to.len() + (tail_end - tail_start) < MAXWLEN
                    {
                        query
                            .bytes
                            .copy_within(tail_start..tail_end, query_pos + to.len());
                        repextra += to.len() as i32 - from.len() as i32;
                        query_len = (initial_query_len as i32 + repextra) as u8;
                    } else {
                        continue;
                    }
                }
                let query_pos = query_pos as usize;
                query.bytes[query_pos..query_pos + to.len()].copy_from_slice(to_bytes);

                recurse!(SCORE_REP);

                current.query_min_pos = (query_pos + to.len()) as u8;
            }

            State::RepUndo => {
                let query_pos = current.query_pos as usize;
                let rep_cursor = (current.cursor - 1) as usize;

                if let Some(rep) = dict.rep.get(rep_cursor) {
                    let from_len = rep.from.len();
                    let to_len = rep.to.len();

                    if from_len != to_len {
                        let tail_start = query_pos + to_len;
                        let tail_end = query_len as usize;
                        if tail_start <= tail_end {
                            query
                                .bytes
                                .copy_within(tail_start..tail_end, query_pos + from_len);
                            repextra -= to_len as i32 - from_len as i32;
                            query_len = (initial_query_len as i32 + repextra) as u8;
                        }
                    }

                    let from_bytes = &dict.arena[rep.from];
                    query.bytes[query_pos..query_pos + from_len].copy_from_slice(from_bytes);
                }

                state = State::Rep;
            }

            State::RepsalIni => {
                if dict.repsal.is_empty()
                    || current.score + SCORE_REP >= max_score
                    || current.query_pos < current.query_min_pos
                {
                    state = State::Final;
                    continue;
                }

                let query_pos = current.query_pos;
                if query_pos >= query_len {
                    state = State::Final;
                    continue;
                }

                let first = dict.repsal_first[query[query_pos] as usize];
                if first < 0 {
                    state = State::Final;
                    continue;
                }

                current.cursor = first;
                state = State::Repsal;
            }

            State::Repsal => {
                let query_pos = current.query_pos;
                let rep_cursor = current.cursor as usize;

                let Some(&RepItem { from, to }) = dict.repsal.get(rep_cursor) else {
                    state = State::Final;
                    continue;
                };

                let from_bytes = &dict.arena[from];
                if from_bytes[0] != query[query_pos] {
                    state = State::Final;
                    continue;
                }

                current.cursor += 1;

                if (query_pos as usize) + from.len() > query_len as usize {
                    continue;
                }

                if query.bytes[(query_pos as usize)..(query_pos as usize) + from.len()]
                    != *from_bytes
                {
                    continue;
                }

                if current.score + SCORE_REP >= max_score {
                    continue;
                }

                let to_bytes = &dict.arena[to];

                state = State::RepsalUndo;
                current.state = state;

                if from.len() != to.len() {
                    let query_pos = query_pos as usize;
                    let tail_start = query_pos + from.len();
                    let tail_end = query_len as usize;
                    if tail_start <= tail_end
                        && query_pos + to.len() + (tail_end - tail_start) < MAXWLEN
                    {
                        query
                            .bytes
                            .copy_within(tail_start..tail_end, query_pos + to.len());
                        repextra += to.len() as i32 - from.len() as i32;
                        query_len = (initial_query_len as i32 + repextra) as u8;
                    } else {
                        continue;
                    }
                }
                let query_pos = query_pos as usize;
                query.bytes[query_pos..query_pos + to.len()].copy_from_slice(to_bytes);

                recurse!(SCORE_REP);
                current.query_min_pos = (query_pos + to.len()) as u8;
            }

            State::RepsalUndo => {
                let query_pos = current.query_pos as usize;
                let curi = (current.cursor - 1) as usize;
                if let Some(&RepItem { from, to }) = dict.repsal.get(curi) {
                    if from.len() != to.len() {
                        let tail_start = query_pos + to.len();
                        let tail_end = query_len as usize;
                        if tail_start <= tail_end {
                            query
                                .bytes
                                .copy_within(tail_start..tail_end, query_pos + from.len());
                            repextra -= to.len() as i32 - from.len() as i32;
                            query_len = (initial_query_len as i32 + repextra) as u8;
                        }
                    }

                    let from_bytes = &dict.arena[from];
                    query.bytes[query_pos..query_pos + from.len()].copy_from_slice(from_bytes);
                }

                state = State::Repsal;
            }

            State::Final => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                state = stack[depth as usize].state;
                current = &mut stack[depth as usize];
            }
        }
    }
}
