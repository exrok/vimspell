use hashbrown::HashMap;

use crate::MAXWLEN_EXT;

use super::Dictionary;
use super::MAXWLEN;
use super::SCORE_DEL;
use super::SCORE_DELDUP;
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
use super::WF_BANNED;
use super::WF_NEEDCOMP;
use super::WF_NOSUGGEST;
use super::WF_RARE;
use super::WF_REGION;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum State {
    Start,
    EndNul,
    Plain,
    Del,
    InsPrep,
    Ins,
    Swap,
    Unswap,
    Swap3,
    Unswap3,
    Rot3l,
    UnRot3l,
    Rot3r,
    UnRot3r,
    RepIni,
    Rep,
    RepUndo,
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
        }
    }
}

fn go_deeper(stack: &mut [TryState; MAXWLEN_EXT], depth: usize, score_add: i32) {
    let parent = stack[depth];
    let child = &mut stack[depth + 1];
    *child = parent;
    child.state = State::Start;
    child.score = parent.score + score_add;
    child.curi = 1;
    child.flags = 0;
}

fn can_go_deeper(stack: &[TryState; MAXWLEN_EXT], depth: usize, add: i32, maxscore: i32) -> bool {
    depth < MAXWLEN - 1 && stack[depth].score + add < maxscore
}

fn add_suggestion(scored: &mut HashMap<Vec<u8>, i32>, word: &[u8], score: i32) {
    match scored.entry_ref(word) {
        hashbrown::hash_map::EntryRef::Occupied(mut occupied_entry) => {
            let existing_score = occupied_entry.get_mut();
            if score < *existing_score {
                *existing_score = score;
            }
            return;
        }
        hashbrown::hash_map::EntryRef::Vacant(vacant_entry_ref) => {
            vacant_entry_ref.insert(score);
            return;
        }
    }
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

pub(crate) fn suggest_trie_walk(
    dict: &Dictionary,
    fword: &mut FWord,
    fword_len: u8,
    scored: &mut HashMap<Vec<u8>, i32>,
    maxscore: i32,
) {
    if dict.foldtree.byts.is_empty() {
        return;
    }

    let byts = &dict.foldtree.byts;
    let idxs = &dict.foldtree.idxs;

    debug_assert_eq!(
        MAXWLEN_EXT,
        u8::MAX as usize,
        "u8::MAX size implies u8 index always succeeds removing branch"
    );
    let mut tword = [0u8; MAXWLEN_EXT];
    let mut stack = [TryState::default(); MAXWLEN_EXT];
    let mut depth: u8 = 0;
    let mut repextra: i32 = 0;
    let mut eff_fword_len = fword_len;

    loop {
        let d = depth as usize;
        match stack[d].state {
            State::Start => {
                let arridx = stack[d].arridx as usize;
                let Some(&len_byte) = byts.get(arridx) else {
                    depth -= 1;
                    continue;
                };
                let len = len_byte as i16;
                let cur = stack[d].curi;

                if cur > len || byts[arridx + cur as usize] != 0 {
                    stack[d].state = State::EndNul;
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

                let goodword_ends = !(fword_ends && (flags as u16 & WF_NEEDCOMP != 0));

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

                if fword_ends && goodword_ends && stack[d].fidx >= stack[d].fidxtry {
                    let total = stack[d].score + newscore;
                    if total < maxscore {
                        let split_off = stack[d].split_tword_off as usize;
                        if split_off > 0 {
                            let mut word = Vec::with_capacity(tword_len + 1);
                            word.extend_from_slice(&tword[..split_off]);
                            word.push(b' ');
                            word.extend_from_slice(&tword[split_off..tword_len]);
                            add_suggestion(scored, &word, total);
                        } else {
                            add_suggestion(scored, &tword[..tword_len], total);
                        }
                    }
                }

                if !fword_ends
                    && goodword_ends
                    && stack[d].split_tword_off == 0
                    && tword_len >= 2
                    && (eff_fword_len - fidx) >= 2
                    && stack[d].fidx >= stack[d].fidxtry
                {
                    let extra = newscore + SCORE_SPLIT;
                    if can_go_deeper(&stack, d, extra, maxscore) {
                        go_deeper(&mut stack, d, extra);
                        depth += 1;
                        let sd = depth as usize;
                        stack[sd].arridx = 0;
                        stack[sd].split_tword_off = tword_len as u8;
                        stack[sd].split_fidx = stack[d].fidx;
                        continue;
                    }
                }
            }

            State::EndNul => {
                if stack[d].fidx >= eff_fword_len {
                    stack[d].state = State::Del;
                } else {
                    stack[d].state = State::Plain;
                }
            }

            State::Plain => {
                let arridx = stack[d].arridx as usize;
                let Some(&len_byte) = byts.get(arridx) else {
                    stack[d].state = State::Final;
                    continue;
                };
                let len = len_byte as i16;

                if stack[d].curi > len {
                    stack[d].state = if stack[d].fidx >= stack[d].fidxtry {
                        State::Del
                    } else {
                        State::Final
                    };
                    continue;
                }

                let idx = arridx + stack[d].curi as usize;
                stack[d].curi += 1;
                let c = byts[idx];

                let fidx = stack[d].fidx;
                let newscore = if fidx < eff_fword_len && c == fword[fidx] {
                    0
                } else if fidx >= eff_fword_len {
                    SCORE_INS
                } else if dict.similar_chars(fword[fidx], c) {
                    SCORE_SIMILAR
                } else {
                    SCORE_SUBST
                };

                if newscore != 0
                    && (stack[d].fidx < stack[d].fidxtry
                        || ((stack[d].flags & TSF_DIDDEL) != 0 && c == fword[stack[d].delidx]))
                {
                    continue;
                }

                if can_go_deeper(&stack, d, newscore, maxscore) {
                    go_deeper(&mut stack, d, newscore);
                    depth += 1;
                    let sd = depth as usize;
                    if fidx < eff_fword_len {
                        stack[sd].fidx += 1;
                    }
                    tword[stack[sd].tword_len as usize] = c;
                    stack[sd].tword_len += 1;
                    stack[sd].arridx = idxs[idx];

                    if newscore == SCORE_SUBST && fidx < eff_fword_len {
                        if dict.similar_chars(fword[fidx], c) {
                            stack[sd].score -= SCORE_SUBST - SCORE_SIMILAR;
                        }
                    }
                }
            }

            State::Del => {
                stack[d].state = State::InsPrep;
                stack[d].curi = 1;

                let fidx = stack[d].fidx;
                if fidx >= eff_fword_len || stack[d].fidx < stack[d].fidxtry {
                    continue;
                }

                let newscore = if fidx > 0 && fword[fidx] == fword[fidx - 1] {
                    SCORE_DELDUP
                } else {
                    SCORE_DEL
                };

                if can_go_deeper(&stack, d, newscore, maxscore) {
                    go_deeper(&mut stack, d, newscore);
                    depth += 1;
                    let sd = depth as usize;
                    stack[sd].flags |= TSF_DIDDEL;
                    stack[sd].delidx = stack[d].fidx;
                    stack[sd].fidx += 1;

                    let new_fidx = stack[sd].fidx;
                    if new_fidx < eff_fword_len && fword[new_fidx] == fword[fidx] {
                        stack[sd].score -= SCORE_DEL - SCORE_DELDUP;
                    }
                }
            }

            State::InsPrep => {
                if stack[d].flags & TSF_DIDDEL != 0 {
                    stack[d].state = State::Swap;
                    continue;
                }

                let arridx = stack[d].arridx as usize;
                let Some(&len_byte) = byts.get(arridx) else {
                    stack[d].state = State::Swap;
                    continue;
                };
                let len = len_byte as i16;

                loop {
                    if stack[d].curi > len {
                        stack[d].state = State::Swap;
                        break;
                    }
                    if byts[arridx + stack[d].curi as usize] != 0 {
                        stack[d].state = State::Ins;
                        break;
                    }
                    stack[d].curi += 1;
                }
            }

            State::Ins => {
                let arridx = stack[d].arridx as usize;
                let Some(&len_byte) = byts.get(arridx) else {
                    stack[d].state = State::Swap;
                    continue;
                };
                let len = len_byte as i16;

                if stack[d].curi > len {
                    stack[d].state = State::Swap;
                    continue;
                }

                let idx = arridx + stack[d].curi as usize;
                stack[d].curi += 1;

                let Some(&c) = byts.get(idx) else {
                    stack[d].state = State::Swap;
                    continue;
                };

                if c == 0 {
                    continue;
                }

                let fidx = stack[d].fidx;
                if fidx < eff_fword_len && c == fword[fidx] {
                    continue;
                }

                if can_go_deeper(&stack, d, SCORE_INS, maxscore) {
                    go_deeper(&mut stack, d, SCORE_INS);
                    depth += 1;
                    let sd = depth as usize;
                    tword[stack[sd].tword_len as usize] = c;
                    stack[sd].tword_len += 1;
                    stack[sd].arridx = idxs[idx];

                    let tw_len = stack[sd].tword_len as usize;
                    if tw_len >= 2 && tword[tw_len - 2] == c {
                        stack[sd].score -= SCORE_INS - SCORE_INSDUP;
                    }
                }
            }

            State::Swap => {
                let fidx = stack[d].fidx;
                if fidx + 1 >= eff_fword_len || stack[d].fidx < stack[d].fidxtry {
                    stack[d].state = State::RepIni;
                    continue;
                }

                let c1 = fword[fidx];
                let c2 = fword[fidx + 1];

                if c1 == c2 {
                    stack[d].state = State::Swap3;
                    continue;
                }

                if can_go_deeper(&stack, d, SCORE_SWAP, maxscore) {
                    stack[d].state = State::Unswap;
                    fword[fidx] = c2;
                    fword[fidx + 1] = c1;
                    go_deeper(&mut stack, d, SCORE_SWAP);
                    depth += 1;
                    stack[depth as usize].fidxtry = stack[d].fidx + 2;
                } else {
                    stack[d].state = State::RepIni;
                }
            }

            State::Unswap => {
                let fidx = stack[d].fidx;
                let c1 = fword[fidx];
                let c2 = fword[fidx + 1];
                fword[fidx] = c2;
                fword[fidx + 1] = c1;
                stack[d].state = State::Swap3;
            }

            State::Swap3 => {
                let fidx = stack[d].fidx;
                if fidx + 2 >= eff_fword_len || stack[d].fidx < stack[d].fidxtry {
                    stack[d].state = State::RepIni;
                    continue;
                }

                let c1 = fword[fidx];
                let c3 = fword[fidx + 2];

                if c1 == c3 {
                    stack[d].state = State::RepIni;
                    continue;
                }

                if can_go_deeper(&stack, d, SCORE_SWAP3, maxscore) {
                    stack[d].state = State::Unswap3;
                    fword[fidx] = c3;
                    fword[fidx + 2] = c1;
                    go_deeper(&mut stack, d, SCORE_SWAP3);
                    depth += 1;
                    stack[depth as usize].fidxtry = stack[d].fidx + 3;
                } else {
                    stack[d].state = State::RepIni;
                }
            }

            State::Unswap3 => {
                let fidx = stack[d].fidx;
                let c1 = fword[fidx];
                let c3 = fword[fidx + 2];
                fword[fidx] = c3;
                fword[fidx + 2] = c1;

                if fidx + 2 < eff_fword_len {
                    stack[d].state = State::Rot3l;
                } else {
                    stack[d].state = State::RepIni;
                }
            }

            State::Rot3l => {
                let fidx = stack[d].fidx;
                if can_go_deeper(&stack, d, SCORE_SWAP3, maxscore) {
                    stack[d].state = State::UnRot3l;
                    let a = fword[fidx];
                    let b = fword[fidx + 1];
                    let c = fword[fidx + 2];
                    fword[fidx] = b;
                    fword[fidx + 1] = c;
                    fword[fidx + 2] = a;
                    go_deeper(&mut stack, d, SCORE_SWAP3);
                    depth += 1;
                    stack[depth as usize].fidxtry = stack[d].fidx + 3;
                } else {
                    stack[d].state = State::RepIni;
                }
            }

            State::UnRot3l => {
                let fidx = stack[d].fidx;
                let b = fword[fidx];
                let c = fword[fidx + 1];
                let a = fword[fidx + 2];
                fword[fidx] = a;
                fword[fidx + 1] = b;
                fword[fidx + 2] = c;

                stack[d].state = State::Rot3r;
            }

            State::Rot3r => {
                let fidx = stack[d].fidx;
                if can_go_deeper(&stack, d, SCORE_SWAP3, maxscore) {
                    stack[d].state = State::UnRot3r;
                    let a = fword[fidx];
                    let b = fword[fidx + 1];
                    let c = fword[fidx + 2];
                    fword[fidx] = c;
                    fword[fidx + 1] = a;
                    fword[fidx + 2] = b;
                    go_deeper(&mut stack, d, SCORE_SWAP3);
                    depth += 1;
                    stack[depth as usize].fidxtry = stack[d].fidx + 3;
                } else {
                    stack[d].state = State::RepIni;
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

                stack[d].state = State::RepIni;
            }

            State::RepIni => {
                if dict.rep.is_empty()
                    || stack[d].score + SCORE_REP >= maxscore
                    || stack[d].fidx < stack[d].fidxtry
                {
                    stack[d].state = State::Final;
                    continue;
                }

                let fidx = stack[d].fidx;
                if fidx >= eff_fword_len {
                    stack[d].state = State::Final;
                    continue;
                }

                let first = dict.rep_first[fword[fidx] as usize];
                if first < 0 {
                    stack[d].state = State::Final;
                    continue;
                }

                stack[d].curi = first as i16;
                stack[d].state = State::Rep;
            }

            State::Rep => {
                let fidx = stack[d].fidx;
                let curi = stack[d].curi as usize;

                if curi >= dict.rep.len() {
                    stack[d].state = State::Final;
                    continue;
                }

                let from_len = dict.rep[curi].from.len();
                let to_len = dict.rep[curi].to.len();
                let first_byte = dict.arena[dict.rep[curi].from][0];

                if first_byte != fword[fidx] {
                    stack[d].state = State::Final;
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

                if !can_go_deeper(&stack, d, SCORE_REP, maxscore) {
                    continue;
                }

                let to_bytes = &dict.arena[dict.rep[curi].to];

                stack[d].state = State::RepUndo;

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
                fword.0[fidx..fidx + to_len].copy_from_slice(&to_bytes);

                go_deeper(&mut stack, d, SCORE_REP);
                depth += 1;
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
                    let fidx = fidx as usize;
                    fword.0[fidx..fidx + from_len].copy_from_slice(&from_bytes);
                }

                stack[d].state = State::Rep;
            }

            State::Final => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
        }
    }
}
