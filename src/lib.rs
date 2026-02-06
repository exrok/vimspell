//! # vim-spell: High performance spell-check with vim's spl dictionary support.

use hashbrown::HashMap;

use crate::suggest::FWord;
mod parser;
mod soundfold;
mod suggest;
#[cfg(test)]
mod tests;

const VIMSPELLMAGIC: &[u8; 8] = b"VIMspell";
const VIMSPELLVERSION: u8 = 50;

/// Maximum word length in bytes. Matches neovim's MAXWLEN.
const MAXWLEN: usize = 254;
/// Maximum world extended to u8::MAX, as an optimization
const MAXWLEN_EXT: usize = 255;

const SN_REGION: u8 = 0;
const SN_CHARFLAGS: u8 = 1;
const SN_MIDWORD: u8 = 2;
const SN_PREFCOND: u8 = 3;
const SN_COMPOUND: u8 = 8;
const SN_SYLLABLE: u8 = 9;
const SN_REP: u8 = 4;
const SN_SAL: u8 = 5;
const SN_MAP: u8 = 7;
const SN_NOBREAK: u8 = 10;
const SN_REPSAL: u8 = 12;
const SN_WORDS: u8 = 13;
const SN_END: u8 = 255;

const SNF_REQUIRED: u8 = 1;

const SAL_F0LLOWUP: u8 = 1;
const SAL_COLLAPSE: u8 = 2;
const SAL_REM_ACCENTS: u8 = 4;

const SCORE_SIMILAR: i32 = 33;
const SCORE_REP: i32 = 65;
const SCORE_SWAP: i32 = 75;
const SCORE_SWAP3: i32 = 110;
const SCORE_SUBST: i32 = 93;
const SCORE_DEL: i32 = 94;
const SCORE_DELDUP: i32 = 66;
const SCORE_INS: i32 = 96;
const SCORE_INSDUP: i32 = 67;
const SCORE_SPLIT: i32 = 149;
#[allow(dead_code)]
const SCORE_ICASE: i32 = 52;
const SCORE_RARE: i32 = 180;
const SCORE_REGION: i32 = 200;
const SCORE_MAXINIT: i32 = 350;
const SCORE_MAXMAX: i32 = 999999;

const SCORE_COMMON1: i32 = 30;
const SCORE_COMMON2: i32 = 40;
const SCORE_COMMON3: i32 = 50;
const SCORE_THRES2: u16 = 10;
const SCORE_THRES3: u16 = 100;

const REGION_ALL: u8 = 0xff;

const WF_REGION: u8 = 0x01;
const WF_ONECAP: u8 = 0x02;
const WF_ALLCAP: u8 = 0x04;
const WF_RARE: u8 = 0x08;
const WF_BANNED: u8 = 0x10;
const WF_AFX: u8 = 0x20;
#[allow(dead_code)]
const WF_FIXCAP: u8 = 0x40;
const WF_KEEPCAP: u8 = 0x80;

const WF_HAS_AFF: u16 = 0x0100;
const WF_NEEDCOMP: u16 = 0x0200;
#[allow(dead_code)]
const WF_NOSUGGEST: u16 = 0x0400;
const WF_COMPROOT: u16 = 0x0800;
const WF_NOCOMPBEF: u16 = 0x1000;
const WF_NOCOMPAFT: u16 = 0x2000;

const WFP_RARE: u32 = 0x01;
const WFP_NC: u32 = 0x02;
const WF_RAREPFX: u32 = WFP_RARE << 24;
const WF_PFX_NC: u32 = WFP_NC << 24;

#[allow(dead_code)]
const COMP_CHECKDUP: u8 = 1;
#[allow(dead_code)]
const COMP_CHECKREP: u8 = 2;
#[allow(dead_code)]
const COMP_CHECKCASE: u8 = 4;
#[allow(dead_code)]
const COMP_CHECKTRIPLE: u8 = 8;

const CF_WORD: u8 = 0x01;
const CF_UPPER: u8 = 0x02;

const BY_NOFLAGS: u8 = 0;
const BY_INDEX: u8 = 1;
const BY_FLAGS: u8 = 2;
const BY_FLAGS2: u8 = 3;
const BY_SPECIAL: u8 = BY_FLAGS2;

/// Error type for spell file parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedEof,
    InvalidMagic,
    UnsupportedVersion,
    InvalidSiblingCount,
    TreeIndexOverflow,
    InvalidSharedIndex,
    UnknownRequiredSection,
}

pub(crate) struct WordTree {
    pub(crate) byts: Vec<u8>,
    pub(crate) idxs: Vec<u32>,
}

struct SyllableItem {
    chars: Bytes,
}

struct CompoundRules {
    rules: Vec<Bytes>,
    start_flags: Vec<u8>,
    all_flags: Vec<u8>,
}

impl CompoundRules {
    fn new() -> Self {
        Self {
            rules: Vec::new(),
            start_flags: Vec::new(),
            all_flags: Vec::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    fn flag_allowed_at_start(&self, flag: u8) -> bool {
        self.start_flags.contains(&flag)
    }

    fn flag_allowed(&self, flag: u8) -> bool {
        self.all_flags.contains(&flag)
    }

    fn matches_partial(&self, arena: &Arena, flags: &[u8]) -> bool {
        if self.rules.is_empty() {
            return false;
        }
        for rule in &self.rules {
            if self.rule_matches_partial(&arena[*rule], flags) {
                return true;
            }
        }
        false
    }

    fn matches_complete(&self, arena: &Arena, flags: &[u8]) -> bool {
        if self.rules.is_empty() {
            return false;
        }
        for rule in &self.rules {
            if self.rule_matches_complete(&arena[*rule], flags) {
                return true;
            }
        }
        false
    }

    fn rule_matches_partial(&self, rule: &[u8], flags: &[u8]) -> bool {
        let mut rule_pos = 0;
        for &flag in flags {
            if rule_pos >= rule.len() {
                return false;
            }
            if !self.char_matches(rule, &mut rule_pos, flag) {
                return false;
            }
        }
        true
    }

    fn rule_matches_complete(&self, rule: &[u8], flags: &[u8]) -> bool {
        let mut rule_pos = 0;
        for &flag in flags {
            if rule_pos >= rule.len() {
                return false;
            }
            if !self.char_matches(rule, &mut rule_pos, flag) {
                return false;
            }
        }
        self.can_complete(rule, rule_pos)
    }

    fn char_matches(&self, rule: &[u8], pos: &mut usize, flag: u8) -> bool {
        let Some(&c) = rule.get(*pos) else {
            return false;
        };

        match c {
            b'[' => {
                *pos += 1;
                let mut matched = false;
                while *pos < rule.len() {
                    let ch = rule[*pos];
                    if ch == b']' {
                        *pos += 1;
                        break;
                    }
                    if ch == flag {
                        matched = true;
                    }
                    *pos += 1;
                }
                if matched {
                    self.skip_quantifier(rule, pos);
                }
                matched
            }
            b'*' | b'+' | b'?' => false,
            _ => {
                if c == flag {
                    *pos += 1;
                    self.skip_quantifier(rule, pos);
                    true
                } else {
                    false
                }
            }
        }
    }

    fn skip_quantifier(&self, rule: &[u8], pos: &mut usize) {
        if let Some(&c) = rule.get(*pos) {
            if c == b'*' || c == b'+' || c == b'?' {
                *pos += 1;
            }
        }
    }

    fn can_complete(&self, rule: &[u8], pos: usize) -> bool {
        let mut p = pos;
        while p < rule.len() {
            let c = rule[p];
            match c {
                b'*' | b'?' => p += 1,
                b'[' => {
                    p += 1;
                    while p < rule.len() && rule[p] != b']' {
                        p += 1;
                    }
                    if p < rule.len() {
                        p += 1;
                    }
                    if p < rule.len() && (rule[p] == b'*' || rule[p] == b'?') {
                        p += 1;
                    } else {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        true
    }
}

struct Syllable {
    chars: Bytes,
    items: Vec<SyllableItem>,
}

impl Syllable {
    fn new() -> Self {
        Self {
            chars: Bytes::default(),
            items: Vec::new(),
        }
    }

    fn count(&self, arena: &Arena, word: &[u8]) -> usize {
        if self.chars.is_empty() && self.items.is_empty() {
            return 0;
        }

        let chars = &arena[self.chars];
        let mut cnt = 0;
        let mut skip = false;
        let mut pos = 0;

        while pos < word.len() {
            if word[pos] == b' ' {
                cnt = 0;
                pos += 1;
                continue;
            }

            let mut matched_len = 0;
            for item in &self.items {
                let item_chars = &arena[item.chars];
                if item_chars.len() > matched_len && word[pos..].starts_with(item_chars) {
                    matched_len = item_chars.len();
                }
            }

            if matched_len > 0 {
                cnt += 1;
                skip = false;
                pos += matched_len;
            } else {
                let c = word[pos];
                if !chars.contains(&c) {
                    skip = false;
                } else if !skip {
                    cnt += 1;
                    skip = true;
                }
                pos += 1;
            }
        }
        cnt
    }
}

impl WordTree {
    fn new() -> Self {
        Self {
            byts: Vec::new(),
            idxs: Vec::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.byts.is_empty()
    }
}

pub(crate) struct CharFlags {
    flags: [u8; 256],
    foldchars: [u8; 256],
}

impl CharFlags {
    fn new() -> Self {
        let mut flags = [0u8; 256];
        let mut foldchars: [u8; 256] = std::array::from_fn(|i| i as u8);

        for b in b'a'..=b'z' {
            flags[b as usize] = CF_WORD;
        }
        for b in b'A'..=b'Z' {
            flags[b as usize] = CF_WORD | CF_UPPER;
            foldchars[b as usize] = b.to_ascii_lowercase();
        }
        for b in b'0'..=b'9' {
            flags[b as usize] = CF_WORD;
        }
        flags[b'\'' as usize] = CF_WORD;

        Self { flags, foldchars }
    }

    pub(crate) fn is_word_char(&self, b: u8) -> bool {
        self.flags[b as usize] & CF_WORD != 0
    }

    fn is_upper(&self, b: u8) -> bool {
        self.flags[b as usize] & CF_UPPER != 0
    }

    pub(crate) fn fold(&self, b: u8) -> u8 {
        self.foldchars[b as usize]
    }
}

#[derive(Default)]
struct Arena {
    data: Vec<u8>,
}

impl Arena {
    pub fn alloc(&mut self, bytes: &[u8]) -> Bytes {
        let start = self.data.len() as u32;
        let len = bytes.len() as u32;
        self.data.extend_from_slice(bytes);
        Bytes { start, len }
    }
}

impl std::ops::Index<Bytes> for Arena {
    type Output = [u8];

    fn index(&self, index: Bytes) -> &Self::Output {
        let start = index.start as usize;
        let end = start + index.len as usize;
        &self.data[start..end]
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Bytes {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

impl Bytes {
    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }
    fn is_empty(&self) -> bool {
        self.len == 0
    }
}

fn fnv1a(data: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for &b in data {
        hash ^= b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

#[derive(Clone, Copy)]
struct CommonWordEntry {
    word: Bytes,
    count: u16,
}

impl Default for CommonWordEntry {
    fn default() -> Self {
        Self {
            word: Bytes::default(),
            count: 0,
        }
    }
}

pub(crate) struct CommonWords {
    entries: Vec<CommonWordEntry>,
    mask: u32,
}

impl CommonWords {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            mask: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn with_capacity(cap: usize) -> Self {
        if cap == 0 {
            return Self::new();
        }
        let table_size = (cap * 2).next_power_of_two();
        Self {
            entries: vec![CommonWordEntry::default(); table_size],
            mask: (table_size - 1) as u32,
        }
    }

    pub(crate) fn lookup(&self, arena: &Arena, word: &[u8]) -> u16 {
        if self.entries.is_empty() {
            return 0;
        }
        let hash = fnv1a(word);
        let mut idx = (hash & self.mask) as usize;
        loop {
            let entry = self.entries[idx];
            if entry.word.is_empty() {
                return 0;
            }
            if arena[entry.word] == *word {
                return entry.count;
            }
            idx = (idx + 1) & self.mask as usize;
        }
    }

    fn insert(&mut self, arena: &Arena, word: Bytes, count: u16) {
        if self.entries.is_empty() {
            return;
        }
        let key = &arena[word];
        let hash = fnv1a(key);
        let mut idx = (hash & self.mask) as usize;
        loop {
            let entry = &mut self.entries[idx];
            if entry.word.is_empty() {
                entry.word = word;
                entry.count = count;
                return;
            }
            if arena[entry.word] == *key {
                entry.count = entry.count.saturating_add(count);
                return;
            }
            idx = (idx + 1) & self.mask as usize;
        }
    }
}

fn match_prefix_condition(cond: &[u8], word: &[u8]) -> bool {
    let mut ci = 0usize;
    let mut wi = 0usize;

    while ci < cond.len() {
        if wi >= word.len() {
            return false;
        }

        match cond[ci] {
            b'[' => {
                ci += 1;
                let negated = ci < cond.len() && cond[ci] == b'^';
                if negated {
                    ci += 1;
                }
                let mut matched = false;
                while ci < cond.len() && cond[ci] != b']' {
                    if cond[ci] == word[wi] {
                        matched = true;
                    }
                    ci += 1;
                }
                if ci < cond.len() {
                    ci += 1;
                }
                if negated == matched {
                    return false;
                }
            }
            b'.' => {
                ci += 1;
            }
            c => {
                if c != word[wi] {
                    return false;
                }
                ci += 1;
            }
        }
        wi += 1;
    }

    true
}

pub(crate) struct MapInfo {
    pub(crate) map_array: [u32; 256],
    #[allow(dead_code)]
    map_hash: Vec<(char, u32)>,
}

pub(crate) struct RepItem {
    pub(crate) from: Bytes,
    pub(crate) to: Bytes,
}

struct SalItem {
    lead: Vec<char>,
    oneof: Vec<char>,
    rules: Vec<u8>,
    to: Vec<char>,
}

struct SalInfo {
    items: Vec<SalItem>,
    first: [i32; 256],
    followup: bool,
    collapse: bool,
    #[allow(dead_code)]
    rem_accents: bool,
}

/// A loaded spell dictionary.
pub struct Dictionary {
    pub(crate) arena: Arena,
    pub(crate) foldtree: WordTree,
    keeptree: WordTree,
    prefixtree: WordTree,
    pub(crate) charflags: CharFlags,
    regions: Vec<[u8; 2]>,
    pub(crate) region: u8,
    #[allow(dead_code)]
    midword: Bytes,
    prefcond: Vec<Bytes>,
    comp_max: u8,
    comp_minlen: u8,
    comp_sylmax: u8,
    #[allow(dead_code)]
    comp_options: u8,
    comp_rules: CompoundRules,
    comp_patterns: Vec<(Bytes, Bytes)>,
    syllable: Syllable,
    #[allow(dead_code)]
    nobreak: bool,
    sal: Option<SalInfo>,
    pub(crate) map: Option<MapInfo>,
    pub(crate) rep: Vec<RepItem>,
    pub(crate) rep_first: [i16; 256],
    repsal: Vec<RepItem>,
    repsal_first: [i16; 256],
    pub(crate) common_words: CommonWords,
}

/// A detected typo with position information.
///
/// Contains byte offsets into the original input text. Use `word()` with the
/// original input to retrieve the misspelled word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Typo {
    /// Byte offset of the start of the misspelled word.
    pub start: u32,
    /// Byte offset of the end of the misspelled word (exclusive).
    pub end: u32,
}

impl Typo {
    /// Returns the misspelled word as a byte slice from the input text.
    #[inline]
    pub fn word<'a>(&self, input: &'a [u8]) -> &'a [u8] {
        &input[self.start as usize..self.end as usize]
    }
}

#[derive(Debug, PartialEq)]
enum WordResult {
    Valid,
    ValidRare,
    WrongRegion,
    Banned,
    NotFound,
}

impl Dictionary {
    pub fn parse(content: &[u8]) -> Result<Self, ParseError> {
        parser::parse(content)
    }
    /// Check text for spelling errors, returning an iterator of typos.
    pub fn spell_check<'a>(&'a self, input: &'a [u8]) -> impl Iterator<Item = Typo> + 'a {
        SpellCheckIter::new(self, input)
    }

    /// Returns the region names defined in this dictionary.
    ///
    /// Each region is a 2-byte code (e.g., `b"us"`, `b"ca"`, `b"au"`).
    /// The index of each region corresponds to its bit position in the
    /// region bitmask used by `set_region`.
    pub fn region_names(&self) -> &[[u8; 2]] {
        &self.regions
    }

    /// Set the active region for spell checking.
    ///
    /// Words marked as region-specific will only be accepted if they
    /// match this region. Pass a 2-byte region code (e.g., `b"us"`).
    /// If the region is not found in the dictionary, the region is set
    /// to [`REGION_ALL`] (accept all regions).
    pub fn set_region(&mut self, region: &[u8; 2]) {
        for (i, name) in self.regions.iter().enumerate() {
            if name == region {
                self.region = 1 << i;
                return;
            }
        }
        self.region = REGION_ALL;
    }

    /// Clear the active region, accepting words from all regions.
    pub fn clear_region(&mut self) {
        self.region = REGION_ALL;
    }

    pub fn has_sal(&self) -> bool {
        self.sal.is_some()
    }

    /// Returns `true` if the dictionary has MAP data for similar-character scoring.
    pub fn has_map(&self) -> bool {
        self.map.is_some()
    }

    /// Returns `true` if the dictionary has common word frequency data.
    pub fn has_common_words(&self) -> bool {
        !self.common_words.is_empty()
    }

    pub(crate) fn similar_chars(&self, c1: u8, c2: u8) -> bool {
        let Some(map) = &self.map else {
            return false;
        };
        let m1 = map.map_array[c1 as usize];
        if m1 == 0 {
            return false;
        }
        m1 == map.map_array[c2 as usize]
    }

    pub(crate) fn score_wordcount_adj(&self, score: i32, word: &[u8]) -> i32 {
        let count = self.common_words.lookup(&self.arena, word);
        if count == 0 {
            return score;
        }
        let bonus = if count < SCORE_THRES2 {
            SCORE_COMMON1
        } else if count < SCORE_THRES3 {
            SCORE_COMMON2
        } else {
            SCORE_COMMON3
        };
        (score - bonus).max(0)
    }

    fn soundfold(&self, word: &[u8]) -> Vec<u8> {
        let Some(sal) = &self.sal else {
            return Vec::new();
        };
        soundfold::soundfold_wsal(sal, word, &self.charflags)
    }

    /// Get spelling suggestions for a typo.
    ///
    /// Takes the original input text to extract the misspelled word. This allows
    /// for context-aware suggestions in the future.
    pub fn suggestions(&self, typo: &Typo, input: &[u8]) -> Vec<Vec<u8>> {
        let word = typo.word(input);
        if word.is_empty() || word.len() > MAXWLEN {
            return Vec::new();
        }

        let mut scored: HashMap<Vec<u8>, i32> = HashMap::new();
        let word_len = word.len();

        let mut fword = FWord([0u8; MAXWLEN_EXT]);
        for (i, &b) in word.iter().enumerate() {
            fword[i as u8] = self.charflags.fold(b);
        }

        suggest::suggest_trie_walk(self, &mut fword, word_len as u8, &mut scored, SCORE_MAXINIT);

        // Rescore using sound similarity if SAL data is available.
        if self.sal.is_some() {
            // Fold the typo word for soundfolding.
            let mut folded = [0u8; MAXWLEN];
            for (i, &b) in word.iter().enumerate() {
                folded[i] = self.charflags.fold(b);
            }
            let bad_sound = self.soundfold(&folded[..word_len]);

            // Build alternative soundfolds via REPSAL rules.
            let mut repsal_sounds: Vec<Vec<u8>> = Vec::new();
            if !self.repsal.is_empty() && !bad_sound.is_empty() {
                let bslen = bad_sound.len();
                let mut buf = [0u8; MAXWLEN * 2];
                for i in 0..bslen {
                    let first = self.repsal_first[bad_sound[i] as usize];
                    if first < 0 {
                        continue;
                    }
                    let mut ri = first as usize;
                    while ri < self.repsal.len() {
                        let item = &self.repsal[ri];
                        let from = &self.arena[item.from];
                        if from[0] != bad_sound[i] {
                            break;
                        }
                        if i + from.len() <= bslen && bad_sound[i..i + from.len()] == *from {
                            let to = &self.arena[item.to];
                            let new_len = bslen - from.len() + to.len();
                            if new_len <= MAXWLEN {
                                buf[..i].copy_from_slice(&bad_sound[..i]);
                                buf[i..i + to.len()].copy_from_slice(to);
                                buf[i + to.len()..new_len]
                                    .copy_from_slice(&bad_sound[i + from.len()..bslen]);
                                repsal_sounds.push(buf[..new_len].to_vec());
                            }
                        }
                        ri += 1;
                    }
                }
            }

            if !bad_sound.is_empty() {
                for (cand_word, score) in &mut scored {
                    let mut cand_folded = [0u8; MAXWLEN];
                    for (i, &b) in cand_word.iter().enumerate() {
                        cand_folded[i] = self.charflags.fold(b);
                    }
                    let good_sound = self.soundfold(&cand_folded[..cand_word.len()]);
                    let mut sound_score = soundfold::soundalike_score(&good_sound, &bad_sound);
                    for alt in &repsal_sounds {
                        let alt_score = soundfold::soundalike_score(&good_sound, alt);
                        if alt_score < sound_score {
                            sound_score = alt_score;
                        }
                    }
                    let sound_score = if sound_score >= SCORE_MAXMAX {
                        SCORE_INS * 3
                    } else {
                        sound_score
                    };
                    *score = (3 * *score + sound_score) / 4;
                }
            }
        }

        if !self.common_words.is_empty() {
            for (cand_word, score) in &mut scored {
                *score = self.score_wordcount_adj(*score, cand_word);
            }
        }
        // let mut top_ten_scores = [0i32; 10];
        // if scored.len() > 15 {
        //     for score in scored.values() {
        //         for s in &mut top_ten_scores {
        //             if *s < *score {
        //                 *s = *score;
        //                 break;
        //             }
        //         }
        //     }
        // }
        // todo optimize top ten top ten
        // println!("{:#?}", scored);
        let mut scored: Vec<_> = scored.into_iter().collect();
        scored.sort_by_key(|(_, s)| *s);
        scored.truncate(10);
        scored.into_iter().map(|(w, _)| w).collect()
    }

    /// Check if a single word is spelled correctly.
    pub fn check_word(&self, word: &[u8]) -> bool {
        matches!(
            self.check_word_internal(word),
            WordResult::Valid | WordResult::ValidRare
        )
    }

    fn check_word_internal(&self, word: &[u8]) -> WordResult {
        if word.is_empty() || word.len() > MAXWLEN {
            return WordResult::NotFound;
        }

        let mut folded = [0u8; MAXWLEN];
        let mut has_upper = false;
        let mut first_upper = false;
        let mut all_upper = true;

        for (i, &b) in word.iter().enumerate() {
            if self.charflags.is_upper(b) {
                has_upper = true;
                if i == 0 {
                    first_upper = true;
                }
            } else if self.charflags.is_word_char(b) {
                all_upper = false;
            }
            folded[i] = self.charflags.fold(b);
        }
        let folded = &folded[..word.len()];

        if has_upper && !all_upper {
            all_upper = false;
        }

        let mut flags_buf = [0u32; MAXWLEN];
        let mut wrong_region = false;

        if !self.keeptree.is_empty() {
            let flags_count = self.find_word(&self.keeptree, word, &mut flags_buf);
            for &flags in &flags_buf[..flags_count] {
                if flags & (WF_BANNED as u32) != 0 {
                    return WordResult::Banned;
                }
                if flags & (WF_NEEDCOMP as u32) != 0 {
                    continue;
                }
                if flags & (WF_REGION as u32) != 0 && self.region & ((flags >> 16) as u8) == 0 {
                    wrong_region = true;
                    continue;
                }
                if flags & (WF_RARE as u32) != 0 {
                    return WordResult::ValidRare;
                }
                return WordResult::Valid;
            }
        }

        let flags_count = self.find_word(&self.foldtree, folded, &mut flags_buf);
        if flags_count == 0 {
            let prefix_result = self.find_prefix(folded);
            if prefix_result != WordResult::NotFound {
                return prefix_result;
            }
            if !self.comp_rules.is_empty() {
                return self.check_compound(word, folded);
            }
            if wrong_region {
                return WordResult::WrongRegion;
            }
            return WordResult::NotFound;
        }

        for &flags in &flags_buf[..flags_count] {
            if flags & (WF_BANNED as u32) != 0 {
                continue;
            }

            if flags & (WF_KEEPCAP as u32) != 0 {
                continue;
            }

            if flags & (WF_NEEDCOMP as u32) != 0 {
                continue;
            }

            if flags & (WF_ALLCAP as u32) != 0 && !all_upper {
                continue;
            }

            if flags & (WF_ONECAP as u32) != 0 && !first_upper {
                continue;
            }

            if flags & (WF_REGION as u32) != 0 && self.region & ((flags >> 16) as u8) == 0 {
                wrong_region = true;
                continue;
            }

            if flags & (WF_RARE as u32) != 0 {
                return WordResult::ValidRare;
            }

            return WordResult::Valid;
        }

        let prefix_result = self.find_prefix(folded);
        if prefix_result != WordResult::NotFound {
            return prefix_result;
        }

        if !self.comp_rules.is_empty() {
            return self.check_compound(word, folded);
        }

        if wrong_region {
            return WordResult::WrongRegion;
        }

        WordResult::NotFound
    }

    fn check_compound(&self, word: &[u8], folded: &[u8]) -> WordResult {
        let mut comp_flags = [0u8; MAXWLEN];
        if self.find_compound(folded, 0, 0, &mut comp_flags) {
            return WordResult::Valid;
        }
        if !self.keeptree.is_empty() && self.find_compound(word, 0, 0, &mut comp_flags) {
            return WordResult::Valid;
        }
        WordResult::NotFound
    }

    fn find_compound(
        &self,
        word: &[u8],
        start_offset: usize,
        comp_len: usize,
        comp_flags: &mut [u8],
    ) -> bool {
        if comp_len >= self.comp_max as usize {
            return false;
        }

        let remaining = &word[start_offset..];
        if remaining.is_empty() {
            return false;
        }

        let tree = if start_offset == 0 {
            &self.foldtree
        } else {
            &self.foldtree
        };

        for end_pos in 1..=remaining.len() {
            let part = &remaining[..end_pos];
            if part.len() < self.comp_minlen as usize {
                continue;
            }

            let mut flags_buf = [0u32; MAXWLEN];
            let flags_count = self.find_word(tree, part, &mut flags_buf);

            for &flags in &flags_buf[..flags_count] {
                if flags & (WF_BANNED as u32) != 0 {
                    continue;
                }

                if flags & (WF_REGION as u32) != 0 && self.region & ((flags >> 16) as u8) == 0 {
                    continue;
                }

                let comp_flag = (flags >> 24) as u8;
                if comp_flag == 0 {
                    continue;
                }

                if comp_len > 0 && (flags & (WF_NOCOMPBEF as u32)) != 0 {
                    continue;
                }

                let word_ends = start_offset + end_pos == word.len();

                if !word_ends && (flags & (WF_NOCOMPAFT as u32)) != 0 {
                    continue;
                }

                let allowed = if comp_len == 0 {
                    self.comp_rules.flag_allowed_at_start(comp_flag)
                } else {
                    self.comp_rules.flag_allowed(comp_flag)
                };

                if !allowed {
                    continue;
                }

                if self.check_compound_pattern(word, start_offset + end_pos) {
                    continue;
                }

                comp_flags[comp_len] = comp_flag;

                if word_ends {
                    if !self
                        .comp_rules
                        .matches_complete(&self.arena, &comp_flags[..comp_len + 1])
                    {
                        continue;
                    }

                    if self.comp_sylmax < MAXWLEN as u8 {
                        let syl_count = self.syllable.count(&self.arena, word);
                        if syl_count > self.comp_sylmax as usize {
                            if comp_len + 1 >= self.comp_max as usize {
                                continue;
                            }
                        }
                    }

                    return true;
                }

                if !self
                    .comp_rules
                    .matches_partial(&self.arena, &comp_flags[..comp_len + 1])
                {
                    continue;
                }

                let mut comp_extra = 0;
                if flags & (WF_COMPROOT as u32) != 0 {
                    comp_extra = 1;
                }

                if comp_len + comp_extra + 2 > self.comp_max as usize
                    && self.comp_sylmax == MAXWLEN as u8
                {
                    continue;
                }

                if self.find_compound(word, start_offset + end_pos, comp_len + 1, comp_flags) {
                    return true;
                }
            }
        }

        false
    }

    fn check_compound_pattern(&self, word: &[u8], split_pos: usize) -> bool {
        for &(first, second) in &self.comp_patterns {
            let first_bytes = &self.arena[first];
            let second_bytes = &self.arena[second];
            if first_bytes.len() > split_pos {
                continue;
            }
            let end_of_first = &word[split_pos - first_bytes.len()..split_pos];
            if end_of_first != first_bytes {
                continue;
            }
            let start_of_second = &word[split_pos..];
            if start_of_second.starts_with(second_bytes) {
                return true;
            }
        }
        false
    }

    fn find_word(&self, tree: &WordTree, word: &[u8], out: &mut [u32]) -> usize {
        if tree.is_empty() || word.is_empty() {
            return 0;
        }

        let byts = &tree.byts;
        let idxs = &tree.idxs;

        let mut result_count = 0usize;
        let mut arridx = 0usize;
        let mut wlen = 0usize;

        while arridx < byts.len() {
            let Some(&sibling_count_byte) = byts.get(arridx) else {
                break;
            };
            let sibling_count = sibling_count_byte as usize;
            arridx += 1;

            let start_idx = arridx;

            let mut zero_count = 0;
            while zero_count < sibling_count {
                let Some(&b) = byts.get(arridx + zero_count) else {
                    break;
                };
                if b != 0 {
                    break;
                }
                if wlen == word.len() {
                    if let Some(&flags) = idxs.get(arridx + zero_count) {
                        if result_count < out.len() {
                            out[result_count] = flags;
                            result_count += 1;
                        }
                    }
                }
                zero_count += 1;
            }

            if wlen >= word.len() {
                return result_count;
            }

            let search_byte = word[wlen];
            let search_start = start_idx + zero_count;
            let search_end = start_idx + sibling_count;

            if search_start >= search_end {
                return result_count;
            }

            let mut lo = search_start;
            let mut hi = search_end;
            let mut found_idx = None;

            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                let Some(&mid_byte) = byts.get(mid) else {
                    break;
                };

                if mid_byte == search_byte {
                    found_idx = Some(mid);
                    break;
                } else if mid_byte < search_byte {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }

            let Some(match_idx) = found_idx else {
                return result_count;
            };

            let Some(&next_idx) = idxs.get(match_idx) else {
                break;
            };
            let next_idx = next_idx as usize;
            if next_idx == 0 {
                return result_count;
            }

            arridx = next_idx;
            wlen += 1;
        }

        result_count
    }

    fn find_prefix(&self, folded: &[u8]) -> WordResult {
        if self.prefixtree.is_empty() || folded.is_empty() {
            return WordResult::NotFound;
        }

        let byts = &self.prefixtree.byts;
        let idxs = &self.prefixtree.idxs;

        let mut arridx = 0usize;
        let mut wlen = 0usize;

        loop {
            let Some(&len_byte) = byts.get(arridx) else {
                break;
            };
            let mut len = len_byte as usize;
            arridx += 1;

            if let Some(&b) = byts.get(arridx) {
                if b == 0 {
                    let pref_arridx = arridx;
                    let mut pref_count = 0usize;
                    while pref_count < len {
                        let Some(&pb) = byts.get(arridx + pref_count) else {
                            break;
                        };
                        if pb != 0 {
                            break;
                        }
                        pref_count += 1;
                    }

                    if wlen > 0 && wlen < folded.len() {
                        let result = self.check_prefix_at(folded, wlen, pref_arridx, pref_count);
                        if result != WordResult::NotFound {
                            return result;
                        }
                    }

                    arridx += pref_count;
                    len -= pref_count;

                    if len == 0 {
                        break;
                    }
                }
            }

            if wlen >= folded.len() {
                break;
            }

            let search_byte = folded[wlen];
            let search_end = arridx + len;

            let mut lo = arridx;
            let mut hi = search_end;
            let mut found = false;

            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                let Some(&mid_byte) = byts.get(mid) else {
                    break;
                };
                if mid_byte == search_byte {
                    let next = idxs[mid] as usize;
                    if next == 0 {
                        return WordResult::NotFound;
                    }
                    arridx = next;
                    found = true;
                    break;
                } else if mid_byte < search_byte {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }

            if !found {
                break;
            }

            wlen += 1;
        }

        WordResult::NotFound
    }

    fn check_prefix_at(
        &self,
        folded: &[u8],
        prefix_len: usize,
        pref_arridx: usize,
        pref_count: usize,
    ) -> WordResult {
        let remainder = &folded[prefix_len..];
        let mut flags_buf = [0u32; MAXWLEN];
        let flags_count = self.find_word(&self.foldtree, remainder, &mut flags_buf);
        let mut wrong_region = false;

        for fi in 0..flags_count {
            let word_flags = flags_buf[fi];

            if word_flags & (WF_BANNED as u32) != 0 {
                continue;
            }
            if word_flags & (WF_NEEDCOMP as u32) != 0 {
                continue;
            }

            if word_flags & (WF_REGION as u32) != 0 && self.region & ((word_flags >> 16) as u8) == 0
            {
                wrong_region = true;
                continue;
            }

            let word_affix_id = if word_flags & (WF_AFX as u32) != 0 {
                (word_flags >> 24) as u8
            } else {
                continue;
            };

            for pi in 0..pref_count {
                let pidx = self.prefixtree.idxs[pref_arridx + pi];
                let prefix_affix_id = (pidx & 0xFF) as u8;

                if word_affix_id != prefix_affix_id {
                    continue;
                }

                if (word_flags & (WF_HAS_AFF as u32) != 0) && (pidx & WF_PFX_NC != 0) {
                    continue;
                }

                let condnr = ((pidx >> 8) & 0xFFFF) as usize;
                if condnr < self.prefcond.len() {
                    let cond_bytes = self.prefcond[condnr];
                    if !cond_bytes.is_empty()
                        && !match_prefix_condition(&self.arena[cond_bytes], remainder)
                    {
                        continue;
                    }
                }

                let is_rare = (pidx & WF_RAREPFX != 0) || (word_flags & (WF_RARE as u32) != 0);
                if is_rare {
                    return WordResult::ValidRare;
                }
                return WordResult::Valid;
            }
        }

        if wrong_region {
            return WordResult::WrongRegion;
        }
        WordResult::NotFound
    }

    /// Returns true if compound word support is enabled for this dictionary.
    pub fn has_compound_rules(&self) -> bool {
        !self.comp_rules.is_empty()
    }

    /// Returns compound configuration information for debugging.
    pub fn compound_info(&self) -> CompoundInfo {
        CompoundInfo {
            max_words: self.comp_max,
            min_part_len: self.comp_minlen,
            max_syllables: self.comp_sylmax,
            rules_count: self.comp_rules.rules.len(),
            patterns_count: self.comp_patterns.len(),
            start_flags: self.comp_rules.start_flags.clone(),
            all_flags: self.comp_rules.all_flags.clone(),
        }
    }

    /// Iterates over words with compound flags and calls the callback with (word, flags).
    pub fn iter_compound_words<F>(&self, mut callback: F)
    where
        F: FnMut(&[u8], u32),
    {
        let mut word_buf = [0u8; MAXWLEN];
        self.iter_tree_words(&self.foldtree, &mut word_buf, 0, &mut callback);
    }

    fn iter_tree_words<F>(
        &self,
        tree: &WordTree,
        word_buf: &mut [u8],
        depth: usize,
        callback: &mut F,
    ) where
        F: FnMut(&[u8], u32),
    {
        if tree.is_empty() {
            return;
        }
        self.iter_tree_node(tree, 0, word_buf, depth, callback);
    }

    fn iter_tree_node<F>(
        &self,
        tree: &WordTree,
        arridx: usize,
        word_buf: &mut [u8],
        depth: usize,
        callback: &mut F,
    ) where
        F: FnMut(&[u8], u32),
    {
        let byts = &tree.byts;
        let idxs = &tree.idxs;

        let Some(&sibling_count) = byts.get(arridx) else {
            return;
        };
        let sibling_count = sibling_count as usize;

        for i in 0..sibling_count {
            let idx = arridx + 1 + i;
            let Some(&b) = byts.get(idx) else {
                continue;
            };
            let Some(&flags_or_idx) = idxs.get(idx) else {
                continue;
            };

            if b == 0 {
                let comp_flag = (flags_or_idx >> 24) as u8;
                if comp_flag != 0 {
                    callback(&word_buf[..depth], flags_or_idx);
                }
            } else if depth < word_buf.len() {
                word_buf[depth] = b;
                let child_idx = flags_or_idx as usize;
                if child_idx > 0 && child_idx < byts.len() {
                    self.iter_tree_node(tree, child_idx, word_buf, depth + 1, callback);
                }
            }
        }
    }
}

/// Information about compound word configuration.
#[derive(Debug)]
pub struct CompoundInfo {
    pub max_words: u8,
    pub min_part_len: u8,
    pub max_syllables: u8,
    pub rules_count: usize,
    pub patterns_count: usize,
    pub start_flags: Vec<u8>,
    pub all_flags: Vec<u8>,
}

struct SpellCheckIter<'a> {
    dict: &'a Dictionary,
    input: &'a [u8],
    pos: usize,
}

impl<'a> SpellCheckIter<'a> {
    fn new(dict: &'a Dictionary, input: &'a [u8]) -> Self {
        Self {
            dict,
            input,
            pos: 0,
        }
    }

    #[inline]
    fn skip_non_word(&mut self) {
        while self.pos < self.input.len() {
            let b = self.input[self.pos];
            if self.dict.charflags.is_word_char(b) {
                break;
            }
            self.pos += 1;
        }
    }

    #[inline]
    fn extract_word(&mut self) -> Option<(usize, usize)> {
        self.skip_non_word();

        if self.pos >= self.input.len() {
            return None;
        }

        let start = self.pos;

        while self.pos < self.input.len() {
            let b = self.input[self.pos];
            if !self.dict.charflags.is_word_char(b) {
                break;
            }
            self.pos += 1;
        }

        if self.pos == start {
            return None;
        }

        Some((start, self.pos))
    }
}

impl Iterator for SpellCheckIter<'_> {
    type Item = Typo;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (start, end) = self.extract_word()?;
            let word = &self.input[start..end];

            let is_all_digits = word.iter().all(|&b| b.is_ascii_digit());
            if is_all_digits {
                continue;
            }

            if !self.dict.check_word(word) {
                return Some(Typo {
                    start: start as u32,
                    end: end as u32,
                });
            }
        }
    }
}
