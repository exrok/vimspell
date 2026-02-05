//! # vim-spell: High performance spell-check with vim's spl dictionary support.

const VIMSPELLMAGIC: &[u8; 8] = b"VIMspell";
const VIMSPELLVERSION: u8 = 50;

/// Maximum word length in bytes. Matches neovim's MAXWLEN.
const MAXWLEN: usize = 254;

const SN_REGION: u8 = 0;
const SN_CHARFLAGS: u8 = 1;
const SN_MIDWORD: u8 = 2;
const SN_PREFCOND: u8 = 3;
const SN_END: u8 = 255;

const SNF_REQUIRED: u8 = 1;

const WF_REGION: u8 = 0x01;
const WF_ONECAP: u8 = 0x02;
const WF_ALLCAP: u8 = 0x04;
const WF_RARE: u8 = 0x08;
const WF_BANNED: u8 = 0x10;
const WF_AFX: u8 = 0x20;
const WF_KEEPCAP: u8 = 0x80;

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

#[cfg(test)]
macro_rules! bail {
    ($variant:ident) => {{
        let caller = std::panic::Location::caller();
        println!(
            "ParseError::{} @ {}:{}",
            stringify!($variant),
            caller.file(),
            caller.line()
        );
        return Err(ParseError::$variant);
    }};
}

#[cfg(not(test))]
macro_rules! bail {
    ($variant:ident) => {
        return Err(ParseError::$variant)
    };
}

struct SpellReader<'a> {
    data: &'a [u8],
}

impl<'a> SpellReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    #[track_caller]
    #[inline]
    fn read_u8(&mut self) -> Result<u8, ParseError> {
        let Some((bytes, remainder)) = self.data.split_first_chunk::<1>() else {
            bail!(UnexpectedEof)
        };
        self.data = remainder;
        Ok(bytes[0])
    }

    #[track_caller]
    #[inline]
    fn read_u16_be(&mut self) -> Result<u16, ParseError> {
        let Some((bytes, remainder)) = self.data.split_first_chunk::<2>() else {
            bail!(UnexpectedEof)
        };
        self.data = remainder;
        Ok(u16::from_be_bytes(*bytes))
    }

    #[track_caller]
    #[inline]
    fn read_u24_be(&mut self) -> Result<u32, ParseError> {
        let Some((bytes, remainder)) = self.data.split_first_chunk::<3>() else {
            bail!(UnexpectedEof)
        };
        self.data = remainder;
        Ok(u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]))
    }

    #[track_caller]
    #[inline]
    fn read_u32_be(&mut self) -> Result<u32, ParseError> {
        let Some((bytes, remainder)) = self.data.split_first_chunk::<4>() else {
            bail!(UnexpectedEof)
        };
        self.data = remainder;
        Ok(u32::from_be_bytes(*bytes))
    }

    #[track_caller]
    #[inline]
    fn read_exact(&mut self, n: usize) -> Result<&'a [u8], ParseError> {
        let Some((bytes, remainder)) = self.data.split_at_checked(n) else {
            bail!(UnexpectedEof)
        };
        self.data = remainder;
        Ok(bytes)
    }

    #[track_caller]
    #[inline]
    fn skip(&mut self, n: usize) -> Result<(), ParseError> {
        if self.data.len() < n {
            bail!(UnexpectedEof)
        }
        self.data = &self.data[n..];
        Ok(())
    }
}

struct WordTree {
    byts: Vec<u8>,
    idxs: Vec<u32>,
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

struct CharFlags {
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

    fn is_word_char(&self, b: u8) -> bool {
        self.flags[b as usize] & CF_WORD != 0
    }

    fn is_upper(&self, b: u8) -> bool {
        self.flags[b as usize] & CF_UPPER != 0
    }

    fn fold(&self, b: u8) -> u8 {
        self.foldchars[b as usize]
    }
}

/// A loaded spell dictionary.
pub struct Dictionary {
    foldtree: WordTree,
    keeptree: WordTree,
    #[allow(dead_code)]
    prefixtree: WordTree,
    charflags: CharFlags,
    #[allow(dead_code)]
    regions: Vec<[u8; 2]>,
    #[allow(dead_code)]
    midword: Vec<u8>,
    #[allow(dead_code)]
    prefcond: Vec<Vec<u8>>,
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

#[derive(PartialEq)]
enum WordResult {
    Valid,
    ValidRare,
    Banned,
    NotFound,
}

impl Dictionary {
    /// Parse a spell dictionary from .spl file contents.
    pub fn parse(contents: &[u8]) -> Result<Dictionary, ParseError> {
        let mut r = SpellReader::new(contents);

        let magic = r.read_exact(8)?;
        if magic != VIMSPELLMAGIC {
            bail!(InvalidMagic)
        }

        let version = r.read_u8()?;
        if version != VIMSPELLVERSION {
            bail!(UnsupportedVersion)
        }

        let mut charflags = CharFlags::new();
        let mut regions = Vec::new();
        let mut midword = Vec::new();
        let mut prefcond = Vec::new();

        loop {
            let section_id = r.read_u8()?;
            if section_id == SN_END {
                break;
            }

            let flags = r.read_u8()?;
            let len = r.read_u32_be()? as usize;

            match section_id {
                SN_REGION => {
                    let data = r.read_exact(len)?;
                    for chunk in data.chunks_exact(2) {
                        regions.push([chunk[0], chunk[1]]);
                    }
                }
                SN_CHARFLAGS => {
                    Self::read_charflags(&mut r, len, &mut charflags)?;
                }
                SN_MIDWORD => {
                    midword = r.read_exact(len)?.to_vec();
                }
                SN_PREFCOND => {
                    prefcond = Self::read_prefcond(&mut r, len)?;
                }
                _ => {
                    if flags & SNF_REQUIRED != 0 {
                        bail!(UnknownRequiredSection)
                    }
                    r.skip(len)?;
                }
            }
        }

        let foldtree = Self::read_wordtree(&mut r)?;
        let keeptree = Self::read_wordtree(&mut r)?;
        let prefixtree = Self::read_wordtree(&mut r)?;

        Ok(Dictionary {
            foldtree,
            keeptree,
            prefixtree,
            charflags,
            regions,
            midword,
            prefcond,
        })
    }

    fn read_charflags(
        r: &mut SpellReader,
        len: usize,
        cf: &mut CharFlags,
    ) -> Result<(), ParseError> {
        if len == 0 {
            return Ok(());
        }

        let charflagslen = r.read_u8()? as usize;

        if charflagslen > 0 {
            let flags_data = r.read_exact(charflagslen)?;

            for (i, &flag) in flags_data.iter().enumerate() {
                let idx = 128 + i;
                if idx < 256 {
                    cf.flags[idx] = flag;
                }
            }
        }

        let remaining = len - 1 - charflagslen;
        if remaining < 2 {
            if remaining > 0 {
                r.skip(remaining)?;
            }
            return Ok(());
        }

        let folcharslen = r.read_u16_be()? as usize;
        let to_read = folcharslen.min(remaining - 2);

        if to_read > 0 {
            let folchars = r.read_exact(to_read)?;

            let mut char_idx = 128usize;
            let mut i = 0;
            while i < folchars.len() && char_idx < 256 {
                let b = folchars[i];
                if b < 0x80 {
                    cf.foldchars[char_idx] = b;
                    char_idx += 1;
                    i += 1;
                } else if b < 0xE0 && i + 1 < folchars.len() {
                    cf.foldchars[char_idx] = b;
                    char_idx += 1;
                    i += 2;
                } else if b < 0xF0 && i + 2 < folchars.len() {
                    cf.foldchars[char_idx] = b;
                    char_idx += 1;
                    i += 3;
                } else {
                    char_idx += 1;
                    i += 1;
                }
            }

            let extra = (remaining - 2).saturating_sub(to_read);
            if extra > 0 {
                r.skip(extra)?;
            }
        }

        Ok(())
    }

    fn read_prefcond(r: &mut SpellReader, len: usize) -> Result<Vec<Vec<u8>>, ParseError> {
        if len < 2 {
            r.skip(len)?;
            return Ok(Vec::new());
        }

        let count = r.read_u16_be()? as usize;
        let mut conditions = Vec::with_capacity(count);
        let mut bytes_read = 2;

        for _ in 0..count {
            if bytes_read >= len {
                break;
            }
            let cond_len = r.read_u8()? as usize;
            bytes_read += 1;

            let cond = if cond_len > 0 {
                let data = r.read_exact(cond_len)?;
                bytes_read += cond_len;
                data.to_vec()
            } else {
                Vec::new()
            };
            conditions.push(cond);
        }

        let remaining = len.saturating_sub(bytes_read);
        if remaining > 0 {
            r.skip(remaining)?;
        }

        Ok(conditions)
    }

    fn read_wordtree(r: &mut SpellReader) -> Result<WordTree, ParseError> {
        let node_count = r.read_u32_be()? as usize;

        if node_count == 0 {
            return Ok(WordTree::new());
        }

        let mut byts = vec![0u8; node_count];
        let mut idxs = vec![0u32; node_count];

        Self::read_tree_node(r, &mut byts, &mut idxs, node_count, 0)?;

        Ok(WordTree { byts, idxs })
    }

    const SHARED_MASK: u32 = 0x8000000;

    fn read_tree_node(
        r: &mut SpellReader,
        byts: &mut [u8],
        idxs: &mut [u32],
        maxidx: usize,
        startidx: usize,
    ) -> Result<usize, ParseError> {
        let mut idx = startidx;

        let len = r.read_u8()? as usize;
        if len == 0 {
            bail!(InvalidSiblingCount)
        }

        if startidx + len >= maxidx {
            bail!(TreeIndexOverflow)
        }

        byts[idx] = len as u8;
        idx += 1;

        for _ in 0..len {
            let c = r.read_u8()?;

            if c <= BY_SPECIAL {
                if c == BY_NOFLAGS {
                    idxs[idx] = 0;
                    byts[idx] = 0;
                } else if c == BY_INDEX {
                    let n = r.read_u24_be()?;
                    if n as usize >= maxidx {
                        bail!(InvalidSharedIndex)
                    }
                    idxs[idx] = n | Self::SHARED_MASK;
                    let xbyte = r.read_u8()?;
                    byts[idx] = xbyte;
                } else {
                    let mut flags = if c == BY_FLAGS || c == BY_FLAGS2 {
                        r.read_u8()? as u32
                    } else {
                        0
                    };
                    if c == BY_FLAGS2 {
                        flags |= (r.read_u8()? as u32) << 8;
                    }
                    if flags & (WF_REGION as u32) != 0 {
                        flags |= (r.read_u8()? as u32) << 16;
                    }
                    if flags & (WF_AFX as u32) != 0 {
                        flags |= (r.read_u8()? as u32) << 24;
                    }
                    idxs[idx] = flags;
                    byts[idx] = 0;
                }
            } else {
                byts[idx] = c;
            }
            idx += 1;
        }

        for i in 1..=len {
            let pos = startidx + i;
            if byts[pos] != 0 {
                if idxs[pos] & Self::SHARED_MASK != 0 {
                    idxs[pos] &= !Self::SHARED_MASK;
                } else {
                    idxs[pos] = idx as u32;
                    idx = Self::read_tree_node(r, byts, idxs, maxidx, idx)?;
                }
            }
        }

        Ok(idx)
    }

    /// Check text for spelling errors, returning an iterator of typos.
    pub fn spell_check<'a>(&'a self, input: &'a [u8]) -> impl Iterator<Item = Typo> + 'a {
        SpellCheckIter::new(self, input)
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

        let mut suggestions = Vec::new();
        let mut candidate = [0u8; MAXWLEN + 1];
        let word_len = word.len();

        // Try single-character substitutions
        candidate[..word_len].copy_from_slice(word);
        for i in 0..word_len {
            let original = candidate[i];
            for c in b'a'..=b'z' {
                if c == original {
                    continue;
                }
                candidate[i] = c;
                if self.check_word_internal(&candidate[..word_len]) == WordResult::Valid {
                    let cand_slice = &candidate[..word_len];
                    if !suggestions
                        .iter()
                        .any(|s: &Vec<u8>| s.as_slice() == cand_slice)
                    {
                        suggestions.push(cand_slice.to_vec());
                    }
                }
            }
            candidate[i] = original;
        }

        // Try single-character deletions
        if word_len > 1 {
            for i in 0..word_len {
                candidate[..i].copy_from_slice(&word[..i]);
                candidate[i..word_len - 1].copy_from_slice(&word[i + 1..]);
                let cand_len = word_len - 1;
                if self.check_word_internal(&candidate[..cand_len]) == WordResult::Valid {
                    let cand_slice = &candidate[..cand_len];
                    if !suggestions
                        .iter()
                        .any(|s: &Vec<u8>| s.as_slice() == cand_slice)
                    {
                        suggestions.push(cand_slice.to_vec());
                    }
                }
            }
        }

        // Try single-character insertions
        for i in 0..=word_len {
            candidate[..i].copy_from_slice(&word[..i]);
            candidate[i + 1..=word_len].copy_from_slice(&word[i..]);
            let cand_len = word_len + 1;
            for c in b'a'..=b'z' {
                candidate[i] = c;
                if self.check_word_internal(&candidate[..cand_len]) == WordResult::Valid {
                    let cand_slice = &candidate[..cand_len];
                    if !suggestions
                        .iter()
                        .any(|s: &Vec<u8>| s.as_slice() == cand_slice)
                    {
                        suggestions.push(cand_slice.to_vec());
                    }
                }
            }
        }

        // Try adjacent character transpositions
        if word_len >= 2 {
            candidate[..word_len].copy_from_slice(word);
            for i in 0..word_len - 1 {
                candidate.swap(i, i + 1);
                if self.check_word_internal(&candidate[..word_len]) == WordResult::Valid {
                    let cand_slice = &candidate[..word_len];
                    if !suggestions
                        .iter()
                        .any(|s: &Vec<u8>| s.as_slice() == cand_slice)
                    {
                        suggestions.push(cand_slice.to_vec());
                    }
                }
                candidate.swap(i, i + 1);
            }
        }

        suggestions.truncate(10);
        suggestions
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

        if !self.keeptree.is_empty() {
            let flags_count = self.find_word(&self.keeptree, word, &mut flags_buf);
            for &flags in &flags_buf[..flags_count] {
                if flags & (WF_BANNED as u32) != 0 {
                    return WordResult::Banned;
                }
                if flags & (WF_RARE as u32) != 0 {
                    return WordResult::ValidRare;
                }
                return WordResult::Valid;
            }
        }

        let flags_count = self.find_word(&self.foldtree, folded, &mut flags_buf);
        if flags_count == 0 {
            return WordResult::NotFound;
        }

        for &flags in &flags_buf[..flags_count] {
            if flags & (WF_BANNED as u32) != 0 {
                continue;
            }

            if flags & (WF_KEEPCAP as u32) != 0 {
                continue;
            }

            if flags & (WF_ALLCAP as u32) != 0 && !all_upper {
                continue;
            }

            if flags & (WF_ONECAP as u32) != 0 && !first_upper {
                continue;
            }

            if flags & (WF_RARE as u32) != 0 {
                return WordResult::ValidRare;
            }

            return WordResult::Valid;
        }

        WordResult::NotFound
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

#[cfg(test)]
mod tests {
    use super::*;

    fn load_dict() -> Dictionary {
        let contents = std::fs::read("/code/vim-spell/en.utf-8.spl").expect("should read file");
        Dictionary::parse(&contents).expect("should parse dictionary")
    }

    #[test]
    fn test_parse_dictionary() {
        let dict = load_dict();
        assert!(!dict.foldtree.is_empty());
    }

    #[test]
    fn test_check_valid_words() {
        let dict = load_dict();

        assert!(dict.check_word(b"hello"));
        assert!(dict.check_word(b"world"));
        assert!(dict.check_word(b"the"));
        assert!(dict.check_word(b"is"));
        assert!(dict.check_word(b"a"));
    }

    #[test]
    fn test_check_invalid_words() {
        let dict = load_dict();

        assert!(!dict.check_word(b"asdfgh"));
        assert!(!dict.check_word(b"xyzabc"));
        assert!(!dict.check_word(b"sampl"));
    }

    #[test]
    fn test_spell_check_iter() {
        let dict = load_dict();

        let input = b"This is a sampl text with a typo";
        let typos: Vec<_> = dict.spell_check(input).collect();

        assert!(!typos.is_empty());
        let words: Vec<_> = typos.iter().map(|t| t.word(input)).collect();
        assert!(words.iter().any(|w| *w == b"sampl"));
    }

    #[test]
    fn test_suggestions() {
        let dict = load_dict();

        let input = b"sampl";
        let typo = Typo { start: 0, end: 5 };
        let suggestions = dict.suggestions(&typo, input);

        assert!(suggestions.iter().any(|s| s == b"sample"));
    }

    #[test]
    fn test_typo_zero_copy() {
        let dict = load_dict();

        let input = b"hello wrold goodbye";
        let typos: Vec<_> = dict.spell_check(input).collect();

        assert_eq!(typos.len(), 1);
        assert_eq!(typos[0].start, 6);
        assert_eq!(typos[0].end, 11);
        assert_eq!(typos[0].word(input), b"wrold");
    }
}
