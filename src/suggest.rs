use hashbrown::HashMap;

use crate::MAXWLEN_EXT;

use super::Dictionary;
use super::MAXWLEN;
use super::SCORE_DEL;
use super::SCORE_DELDUP;
use super::SCORE_ICASE;
use super::SCORE_INS;
use super::SCORE_INSDUP;
use super::SCORE_RARE;
use super::SCORE_REGION;
use super::SCORE_REP;
use super::SCORE_SIMILAR;
use super::SCORE_SPLIT;
use super::SCORE_SUBST;
use super::SCORE_SWAP;
use super::SCORE_SWAP3;
use super::WF_ALLCAP;
use super::WF_BANNED;
use super::WF_KEEPCAP;
use super::WF_NEEDCOMP;
use super::WF_NOSUGGEST;
use super::WF_ONECAP;
use super::WF_RARE;
use super::WF_REGION;
use super::WordTree;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum State {
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
struct TryState {
    state: State,
    score: i32,
    arridx: u32,
    curi: i16,
    fidx: u8,
    fidxtry: u8,
    tword_len: u8,
    flags: u8,
    delidx: u8,
    split_tword_off: u8,
    split_fidx: u8,
    split_word_flags: u32,
}

impl Default for TryState {
    fn default() -> Self {
        Self {
            state: State::Start,
            score: 0,
            arridx: 0,
            curi: 1,
            fidx: 0,
            fidxtry: 0,
            tword_len: 0,
            flags: 0,
            delidx: 0,
            split_tword_off: 0,
            split_fidx: 0,
            split_word_flags: 0,
        }
    }
}

/// SAFETY: depth must be < MAXWLEN - 1 (checked by can_go_deeper before every call).
#[inline(always)]
fn go_deeper(stack: &mut [TryState; MAXWLEN_EXT], depth: u8, score_add: i32) {
    debug_assert!(depth + 1 < MAXWLEN_EXT as u8);
    let parent = stack[depth as usize];
    let child = &mut stack[(depth + 1) as usize];
    *child = parent;
    child.state = State::Start;
    child.score = parent.score + score_add;
    child.curi = 1;
    child.flags = 0;
}

#[inline(always)]
fn can_go_deeper(stack: &mut [TryState; MAXWLEN_EXT], depth: u8, add: i32, maxscore: i32) -> bool {
    depth < ((MAXWLEN - 1) as u8) && (stack[depth as usize]).score + add < maxscore
}

const SUG_CLEAN_COUNT: usize = 150;
const SUG_MAX_COUNT: usize = SUG_CLEAN_COUNT + 50;

fn add_suggestion(scored: &mut HashMap<Vec<u8>, i32>, word: &[u8], score: i32) {
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
fn cleanup_suggestions(scored: &mut HashMap<Vec<u8>, i32>, maxscore: i32) -> i32 {
    let mut scores: Vec<i32> = scored.values().copied().collect();
    if scores.len() <= SUG_CLEAN_COUNT {
        return maxscore;
    }
    scores.select_nth_unstable(SUG_CLEAN_COUNT - 1);
    let threshold = scores[SUG_CLEAN_COUNT - 1];
    scored.retain(|_, &mut score| score <= threshold);
    threshold
}
pub struct FWord(pub [u8; MAXWLEN_EXT]);
impl std::ops::Index<u8> for FWord {
    type Output = u8;
    #[inline(always)]
    fn index(&self, index: u8) -> &Self::Output {
        &self.0[index as usize]
    }
}

impl std::ops::IndexMut<u8> for FWord {
    #[inline(always)]
    fn index_mut(&mut self, index: u8) -> &mut Self::Output {
        &mut self.0[index as usize]
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

    let byts = &keeptree.byts;
    let idxs = &keeptree.idxs;
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
            if ai < byts.len() {
                let len = byts[ai] as usize;
                if len > 0 && ai + 1 < byts.len() && byts[ai + 1] == 0 {
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
        if ai >= byts.len() {
            if rnd[d] >= 2 || fword[d] == uword[d] {
                depth -= 1;
            }
            continue;
        }

        let len = byts[ai] as usize;
        let base = ai + 1;

        // Skip NUL entries
        let mut nul_count = 0;
        while nul_count < len && base + nul_count < byts.len() && byts[base + nul_count] == 0 {
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
            if mid >= byts.len() {
                break;
            }
            if byts[mid] == c {
                kword[d] = c;
                arr[d + 1] = idxs[mid] as usize;
                rnd[d + 1] = 0;
                depth += 1;
                found = true;
                break;
            } else if byts[mid] < c {
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
    if word_flags as u8 & WF_KEEPCAP != 0 {
        if let Some(kw) = find_keepcap_word(keeptree, word) {
            out.extend_from_slice(&kw);
            return;
        }
    }
    apply_case(out, word, badflags, word_flags);
}

pub(crate) fn suggest_trie_walk(
    dict: &Dictionary,
    fword: &mut FWord,
    fword_len: u8,
    scored: &mut HashMap<Vec<u8>, i32>,
    maxscore: i32,
    badflags: u8,
) {
    if dict.foldtree.byts.is_empty() {
        return;
    }

    let byts: &[u8] = &dict.foldtree.byts;
    let idxs: &[u32] = &dict.foldtree.idxs;

    // Pre-extract the map_array pointer to avoid Option check per iteration.
    let map_array: Option<&[u32; 256]> = dict.map.as_ref().map(|r| &r.map_array);

    let mut maxscore = maxscore;
    let mut tword = [0u8; MAXWLEN_EXT];
    let mut stack = [TryState::default(); MAXWLEN_EXT];
    let mut depth: u8 = 0;
    let mut repextra: i32 = 0;
    let mut eff_fword_len = fword_len;

    // SAFETY notes for the entire loop:
    // - `depth` is bounded: it only increases via `go_deeper` which requires
    //   `depth < MAXWLEN - 1` (253), so `depth` is always in 0..253 and
    //   `depth as usize` is a valid index into stack[255] and tword[255].
    // - `arridx` comes from idxs[] entries which are valid trie node pointers.
    //   We validate arridx < byts_len before using it.
    // - `arridx + curi` is always <= arridx + len_byte, which is within the
    //   trie node's allocated range in byts/idxs.
    // - `fidx` is bounded by depth + fword_len, always < 255.
    let mut state = State::Start;
    loop {
        let d = depth as usize;
        match state {
            State::Start => {
                let arridx = stack[d].arridx as usize;
                let Some(&len) = byts.get(arridx) else {
                    depth -= 1;
                    state = stack[depth as usize].state;
                    continue;
                };
                let len = len as i16;
                let cur = stack[d].curi;

                // perf: Might not need null check
                if cur > len || byts[arridx + cur as usize] != 0 {
                    if stack[d].fidx >= eff_fword_len {
                        // Inlined Del: fidx >= eff_fword_len is always true
                        // here, so the go_deeper path is unreachable.
                        state = State::InsPrep;
                        stack[d].curi = 1;
                    } else {
                        state = State::Plain;
                    }
                    continue;
                }

                stack[d].curi += 1;

                let flags = idxs[arridx + cur as usize];

                if (flags as u16) & WF_NOSUGGEST != 0 {
                    continue;
                }

                let fidx = stack[d].fidx;
                let fword_ends = fidx >= eff_fword_len;
                let tword_len = stack[d].tword_len as usize;

                let goodword_ends = (flags as u16 & WF_NEEDCOMP) == 0;

                if (flags as u8) & WF_BANNED != 0 {
                    continue;
                }

                let mut newscore: i32 = 0;
                if (flags as u8) & WF_REGION != 0 {
                    let region_mask = ((flags >> 16) & 0xff) as u8;
                    if region_mask & dict.region == 0 {
                        newscore += SCORE_REGION;
                    }
                }
                if (flags as u8) & WF_RARE != 0 {
                    newscore += SCORE_RARE;
                }

                // SCORE_ICASE penalty for case mismatch
                let rct = result_captype(badflags, flags);
                if !spell_valid_case(badflags, rct) {
                    newscore += SCORE_ICASE;
                }

                if fword_ends && goodword_ends && stack[d].fidx >= stack[d].fidxtry {
                    let split_off = stack[d].split_tword_off as usize;
                    let is_split = split_off > 0;

                    // Build cased suggestion word
                    let mut word = Vec::with_capacity(tword_len + 2);
                    if is_split {
                        build_cased_word(
                            &dict.keeptree,
                            &tword[..split_off],
                            badflags,
                            stack[d].split_word_flags,
                            &mut word,
                        );
                        word.push(b' ');
                    }
                    let second_start = word.len();
                    build_cased_word(
                        &dict.keeptree,
                        &tword[if is_split { split_off } else { 0 }..tword_len],
                        badflags,
                        flags,
                        &mut word,
                    );

                    // Apply common word bonus (before SAL, matching Neovim)
                    let total = if !dict.common_words.is_empty() {
                        dict.score_wordcount_adj(
                            stack[d].score + newscore,
                            &word[second_start..],
                            is_split,
                        )
                    } else {
                        stack[d].score + newscore
                    };

                    if total < maxscore {
                        add_suggestion(scored, &word, total);

                        if scored.len() > SUG_MAX_COUNT {
                            maxscore = cleanup_suggestions(scored, maxscore);
                        }
                    }
                }

                if !fword_ends
                    && goodword_ends
                    && stack[d].split_tword_off == 0
                    && stack[d].fidx >= stack[d].fidxtry
                {
                    let mut extra = newscore + SCORE_SPLIT;
                    // Give a bonus to common first words at the split point
                    if !dict.common_words.is_empty() {
                        extra = dict.score_wordcount_adj(extra, &tword[..tword_len], true);
                    }
                    if can_go_deeper(&mut stack, depth, extra, maxscore) {
                        let prev_fidx = stack[d].fidx;
                        stack[d].state = state;
                        go_deeper(&mut stack, depth, extra);
                        depth += 1;
                        state = State::Start;
                        let sd = depth as usize;
                        stack[sd].arridx = 0;
                        stack[sd].split_tword_off = tword_len as u8;
                        stack[sd].split_fidx = prev_fidx;
                        stack[sd].split_word_flags = flags;
                        continue;
                    }
                }
            }
            State::Plain => {
                let arridx = stack[d].arridx as usize;
                let Some(&len) = byts.get(arridx) else {
                    depth -= 1;
                    state = stack[depth as usize].state;
                    continue;
                };
                let len = len as i16;

                if stack[d].curi > len {
                    if stack[d].fidx >= stack[d].fidxtry {
                        // Inlined Del: fidx >= fidxtry is always true
                        // here, so only the eff_fword_len guard remains.
                        state = State::InsPrep;
                        stack[d].curi = 1;

                        let fidx = stack[d].fidx;
                        if fidx < eff_fword_len {
                            let newscore = SCORE_DEL;
                            if can_go_deeper(&mut stack, depth, newscore, maxscore) {
                                stack[d].state = state;
                                go_deeper(&mut stack, depth, newscore);
                                depth += 1;
                                state = State::Start;
                                let sd = depth as usize;
                                stack[sd].flags |= TSF_DIDDEL;
                                stack[sd].delidx = fidx;
                                stack[sd].fidx += 1;

                                let new_fidx = stack[sd].fidx;
                                if new_fidx < eff_fword_len && fword[new_fidx] == fword[fidx] {
                                    stack[sd].score -= SCORE_DEL - SCORE_DELDUP;
                                }
                            }
                        }
                    } else {
                        state = State::Final;
                    }
                    continue;
                }

                let idx = arridx + stack[d].curi as usize;
                stack[d].curi += 1;
                let c = byts[idx];

                let fidx = stack[d].fidx;
                // Match C behavior: always use SCORE_SUBST for pruning.
                // similar_chars adjustment happens post-go_deeper (below).
                let newscore = if fidx < eff_fword_len && c == fword[fidx] {
                    0
                } else if fidx >= eff_fword_len {
                    SCORE_INS
                } else {
                    SCORE_SUBST
                };

                if newscore != 0
                    && (stack[d].fidx < stack[d].fidxtry
                        || ((stack[d].flags & TSF_DIDDEL) != 0 && c == fword[stack[d].delidx]))
                {
                    continue;
                }

                if can_go_deeper(&mut stack, depth, newscore, maxscore) {
                    stack[d].state = state;
                    go_deeper(&mut stack, depth, newscore);
                    depth += 1;
                    state = State::Start;
                    let sd = depth as usize;
                    if fidx < eff_fword_len {
                        stack[sd].fidx += 1;
                    }
                    tword[stack[sd].tword_len as usize] = c;
                    stack[sd].tword_len += 1;
                    stack[sd].arridx = idxs[idx];

                    if newscore == SCORE_SUBST && fidx < eff_fword_len {
                        if similar_chars(map_array, fword[fidx], c) {
                            stack[sd].score -= SCORE_SUBST - SCORE_SIMILAR;
                        }
                    }
                }
            }

            State::InsPrep => {
                if stack[d].flags & TSF_DIDDEL != 0 {
                    state = State::Swap;
                    continue;
                }

                let arridx = stack[d].arridx as usize;
                let Some(&len) = byts.get(arridx) else {
                    depth -= 1;
                    state = stack[depth as usize].state;
                    continue;
                };
                let len = len as i16;

                loop {
                    if stack[d].curi > len {
                        state = State::Swap;
                        break;
                    }
                    if byts[arridx + stack[d].curi as usize] != 0 {
                        state = State::Ins;
                        break;
                    }
                    stack[d].curi += 1;
                }
            }

            State::Ins => {
                let arridx = stack[d].arridx as usize;
                let Some(&len) = byts.get(arridx) else {
                    state = State::Swap;
                    continue;
                };
                let len = len as i16;

                if stack[d].curi > len {
                    state = State::Swap;
                    continue;
                }

                let idx = arridx + stack[d].curi as usize;
                stack[d].curi += 1;

                let Some(&c) = byts.get(idx) else {
                    state = State::Swap;
                    continue;
                };

                if c == 0 {
                    continue;
                }

                let fidx = stack[d].fidx;
                if fidx < eff_fword_len && c == fword[fidx] {
                    continue;
                }

                if can_go_deeper(&mut stack, depth, SCORE_INS, maxscore) {
                    stack[d].state = state;
                    go_deeper(&mut stack, depth, SCORE_INS);
                    depth += 1;
                    state = State::Start;
                    let sd = depth as usize;
                    tword[stack[sd].tword_len as usize] = c;
                    stack[sd].tword_len += 1;
                    stack[sd].arridx = idxs[idx];

                    let tw_len = stack[sd].tword_len;
                    if tw_len >= 2 && tword[(tw_len - 2) as usize] == c {
                        stack[sd].score -= SCORE_INS - SCORE_INSDUP;
                    }
                }
            }

            State::Swap => {
                let fidx = stack[d].fidx;
                if fidx + 1 >= eff_fword_len || stack[d].fidx < stack[d].fidxtry {
                    state = State::RepIni;
                    continue;
                }

                let c1 = fword[fidx];
                let c2 = fword[fidx + 1];

                if c1 == c2 {
                    state = State::Swap3;
                    continue;
                }

                if can_go_deeper(&mut stack, depth, SCORE_SWAP, maxscore) {
                    state = State::Unswap;
                    stack[d].state = state;
                    fword[fidx] = c2;
                    fword[fidx + 1] = c1;
                    let prev_fidx = stack[d].fidx;
                    go_deeper(&mut stack, depth, SCORE_SWAP);
                    depth += 1;
                    state = State::Start;
                    stack[depth as usize].fidxtry = prev_fidx + 2;
                } else {
                    state = State::RepIni;
                }
            }

            State::Unswap => {
                let fidx = stack[d].fidx;
                let c1 = fword[fidx];
                let c2 = fword[fidx + 1];
                fword[fidx] = c2;
                fword[fidx + 1] = c1;
                state = State::Swap3;
            }

            State::Swap3 => {
                let fidx = stack[d].fidx;
                if fidx + 2 >= eff_fword_len || stack[d].fidx < stack[d].fidxtry {
                    state = State::RepIni;
                    continue;
                }

                let c1 = fword[fidx];
                let c3 = fword[fidx + 2];

                if c1 == c3 {
                    state = State::RepIni;
                    continue;
                }

                if can_go_deeper(&mut stack, depth, SCORE_SWAP3, maxscore) {
                    state = State::Unswap3;
                    stack[d].state = state;
                    fword[fidx] = c3;
                    fword[fidx + 2] = c1;
                    let prev_fidx = stack[d].fidx;
                    go_deeper(&mut stack, depth, SCORE_SWAP3);
                    depth += 1;
                    state = State::Start;
                    stack[depth as usize].fidxtry = prev_fidx + 3;
                } else {
                    state = State::RepIni;
                }
            }

            State::Unswap3 => {
                let fidx = stack[d].fidx;
                let c1 = fword[fidx];
                let c3 = fword[fidx + 2];
                fword[fidx] = c3;
                fword[fidx + 2] = c1;

                if fidx + 2 < eff_fword_len {
                    if can_go_deeper(&mut stack, depth, SCORE_SWAP3, maxscore) {
                        state = State::UnRot3l;
                        stack[d].state = state;
                        let a = fword[fidx];
                        let b = fword[fidx + 1];
                        let c = fword[fidx + 2];
                        fword[fidx] = b;
                        fword[fidx + 1] = c;
                        fword[fidx + 2] = a;
                        let prev_fidx = stack[d].fidx;
                        go_deeper(&mut stack, depth, SCORE_SWAP3);
                        depth += 1;
                        state = State::Start;
                        stack[depth as usize].fidxtry = prev_fidx + 3;
                        continue;
                    }
                }
                state = State::RepIni;
            }

            State::UnRot3l => {
                let fidx = stack[d].fidx;
                let b = fword[fidx];
                let c = fword[fidx + 1];
                let a = fword[fidx + 2];
                fword[fidx] = a;
                fword[fidx + 1] = b;
                fword[fidx + 2] = c;

                if can_go_deeper(&mut stack, depth, SCORE_SWAP3, maxscore) {
                    state = State::UnRot3r;
                    stack[d].state = state;
                    let a = fword[fidx];
                    let b = fword[fidx + 1];
                    let c = fword[fidx + 2];
                    fword[fidx] = c;
                    fword[fidx + 1] = a;
                    fword[fidx + 2] = b;
                    let prev_fidx = stack[d].fidx;
                    go_deeper(&mut stack, depth, SCORE_SWAP3);
                    depth += 1;
                    state = State::Start;
                    stack[depth as usize].fidxtry = prev_fidx + 3;
                } else {
                    state = State::RepIni;
                }
            }

            State::UnRot3r => {
                let fidx = stack[d].fidx;
                let c = fword[fidx];
                let a = fword[fidx + 1];
                let b = fword[fidx + 2];
                fword[fidx] = a;
                fword[fidx + 1] = b;
                fword[fidx + 2] = c;

                state = State::RepIni;
            }

            State::RepIni => {
                if dict.rep.is_empty()
                    || stack[d].score + SCORE_REP >= maxscore
                    || stack[d].fidx < stack[d].fidxtry
                {
                    state = State::RepsalIni;
                    continue;
                }

                let fidx = stack[d].fidx;
                if fidx >= eff_fword_len {
                    state = State::RepsalIni;
                    continue;
                }

                let first = dict.rep_first[fword[fidx] as usize];
                if first < 0 {
                    state = State::RepsalIni;
                    continue;
                }

                stack[d].curi = first as i16;
                state = State::Rep;
            }

            State::Rep => {
                let fidx = stack[d].fidx;
                let curi = stack[d].curi as usize;

                if curi >= dict.rep.len() {
                    state = State::RepsalIni;
                    continue;
                }

                let from_len = dict.rep[curi].from.len();
                let to_len = dict.rep[curi].to.len();
                let first_byte = dict.arena[dict.rep[curi].from][0];

                if first_byte != fword[fidx] {
                    state = State::RepsalIni;
                    continue;
                }

                stack[d].curi += 1;

                if (fidx as usize) + from_len > eff_fword_len as usize {
                    continue;
                }

                let from_bytes = &dict.arena[dict.rep[curi].from];
                if fword.0[(fidx as usize)..(fidx as usize) + from_len] != *from_bytes {
                    continue;
                }

                if !can_go_deeper(&mut stack, depth, SCORE_REP, maxscore) {
                    continue;
                }

                let to_bytes = &dict.arena[dict.rep[curi].to];

                state = State::RepUndo;
                stack[d].state = state;

                if from_len != to_len {
                    let fidx = fidx as usize;
                    let tail_start = fidx + from_len;
                    let tail_end = eff_fword_len as usize;
                    if tail_start <= tail_end && fidx + to_len + (tail_end - tail_start) < MAXWLEN {
                        fword.0.copy_within(tail_start..tail_end, fidx + to_len);
                        repextra += to_len as i32 - from_len as i32;
                        eff_fword_len = (fword_len as i32 + repextra) as u8;
                    } else {
                        continue;
                    }
                }
                let fidx = fidx as usize;
                fword.0[fidx..fidx + to_len].copy_from_slice(to_bytes);

                go_deeper(&mut stack, depth, SCORE_REP);
                depth += 1;
                state = State::Start;
                stack[depth as usize].fidxtry = (fidx + to_len) as u8;
            }

            State::RepUndo => {
                let fidx = stack[d].fidx as usize;
                let curi = (stack[d].curi - 1) as usize;

                if curi < dict.rep.len() {
                    let from_len = dict.rep[curi].from.len();
                    let to_len = dict.rep[curi].to.len();
                    let from_bytes = &dict.arena[dict.rep[curi].from];

                    if from_len != to_len {
                        let tail_start = fidx + to_len;
                        let tail_end = eff_fword_len as usize;
                        if tail_start <= tail_end {
                            fword.0.copy_within(tail_start..tail_end, fidx + from_len);
                            repextra -= to_len as i32 - from_len as i32;
                            eff_fword_len = (fword_len as i32 + repextra) as u8;
                        }
                    }
                    fword.0[fidx..fidx + from_len].copy_from_slice(from_bytes);
                }

                state = State::Rep;
            }

            State::RepsalIni => {
                if dict.repsal.is_empty()
                    || stack[d].score + SCORE_REP >= maxscore
                    || stack[d].fidx < stack[d].fidxtry
                {
                    state = State::Final;
                    continue;
                }

                let fidx = stack[d].fidx;
                if fidx >= eff_fword_len {
                    state = State::Final;
                    continue;
                }

                let first = dict.repsal_first[fword[fidx] as usize];
                if first < 0 {
                    state = State::Final;
                    continue;
                }

                stack[d].curi = first as i16;
                state = State::Repsal;
            }

            State::Repsal => {
                let fidx = stack[d].fidx;
                let curi = stack[d].curi as usize;

                if curi >= dict.repsal.len() {
                    state = State::Final;
                    continue;
                }

                let from_len = dict.repsal[curi].from.len();
                let to_len = dict.repsal[curi].to.len();
                let first_byte = dict.arena[dict.repsal[curi].from][0];

                if first_byte != fword[fidx] {
                    state = State::Final;
                    continue;
                }

                stack[d].curi += 1;

                if (fidx as usize) + from_len > eff_fword_len as usize {
                    continue;
                }

                let from_bytes = &dict.arena[dict.repsal[curi].from];
                if fword.0[(fidx as usize)..(fidx as usize) + from_len] != *from_bytes {
                    continue;
                }

                if !can_go_deeper(&mut stack, depth, SCORE_REP, maxscore) {
                    continue;
                }

                let to_bytes = &dict.arena[dict.repsal[curi].to];

                state = State::RepsalUndo;
                stack[d].state = state;

                if from_len != to_len {
                    let fidx = fidx as usize;
                    let tail_start = fidx + from_len;
                    let tail_end = eff_fword_len as usize;
                    if tail_start <= tail_end && fidx + to_len + (tail_end - tail_start) < MAXWLEN {
                        fword.0.copy_within(tail_start..tail_end, fidx + to_len);
                        repextra += to_len as i32 - from_len as i32;
                        eff_fword_len = (fword_len as i32 + repextra) as u8;
                    } else {
                        continue;
                    }
                }
                let fidx = fidx as usize;
                fword.0[fidx..fidx + to_len].copy_from_slice(to_bytes);

                go_deeper(&mut stack, depth, SCORE_REP);
                depth += 1;
                state = State::Start;
                stack[depth as usize].fidxtry = (fidx + to_len) as u8;
            }

            State::RepsalUndo => {
                let fidx = stack[d].fidx as usize;
                let curi = (stack[d].curi - 1) as usize;

                if curi < dict.repsal.len() {
                    let from_len = dict.repsal[curi].from.len();
                    let to_len = dict.repsal[curi].to.len();
                    let from_bytes = &dict.arena[dict.repsal[curi].from];

                    if from_len != to_len {
                        let tail_start = fidx + to_len;
                        let tail_end = eff_fword_len as usize;
                        if tail_start <= tail_end {
                            fword.0.copy_within(tail_start..tail_end, fidx + from_len);
                            repextra -= to_len as i32 - from_len as i32;
                            eff_fword_len = (fword_len as i32 + repextra) as u8;
                        }
                    }
                    fword.0[fidx..fidx + from_len].copy_from_slice(from_bytes);
                }

                state = State::Repsal;
            }

            State::Final => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                state = stack[depth as usize].state;
            }
        }
    }
}
