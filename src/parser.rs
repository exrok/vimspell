use super::*;

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
    fn read_u16_le(&mut self) -> Result<u16, ParseError> {
        let Some((bytes, remainder)) = self.data.split_first_chunk::<2>() else {
            bail!(UnexpectedEof)
        };
        self.data = remainder;
        Ok(u16::from_le_bytes(*bytes))
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
/// Parse a spell dictionary from .spl file contents.
pub fn parse(contents: &[u8]) -> Result<Dictionary, ParseError> {
    let mut r = SpellReader::new(contents);
    let mut a = Arena::default();

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
    let mut midword = Bytes::default();
    let mut prefcond = Vec::new();
    let mut comp_max = MAXWLEN as u8;
    let mut comp_minlen = 0u8;
    let mut comp_sylmax = MAXWLEN as u8;
    let mut comp_options = 0u8;
    let mut comp_rules = CompoundRules::new();
    let mut comp_patterns = Vec::new();
    let mut syllable = Syllable::new();
    let mut nobreak = false;
    let mut sal: Option<SalInfo> = None;
    let mut map: Option<MapInfo> = None;
    let mut rep = Vec::new();
    let mut rep_first = [-1i16; 256];
    let mut repsal = Vec::new();
    let mut repsal_first = [-1i16; 256];
    let mut common_words = CommonWords::new();

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
                read_charflags(&mut r, len, &mut charflags)?;
            }
            SN_MIDWORD => {
                let data = r.read_exact(len)?;
                midword = a.alloc(data);
            }
            SN_PREFCOND => {
                prefcond = read_prefcond(&mut r, &mut a, len)?;
            }
            SN_COMPOUND => {
                read_compound(
                    &mut r,
                    &mut a,
                    len,
                    &mut comp_max,
                    &mut comp_minlen,
                    &mut comp_sylmax,
                    &mut comp_options,
                    &mut comp_rules,
                    &mut comp_patterns,
                )?;
            }
            SN_SYLLABLE => {
                read_syllable(&mut r, &mut a, len, &mut syllable)?;
            }
            SN_REP => {
                read_rep_section(&mut r, &mut a, len, &mut rep, &mut rep_first)?;
            }
            SN_SAL => {
                sal = Some(read_sal_section(&mut r, len)?);
            }
            SN_MAP => {
                map = Some(read_map_section(&mut r, len)?);
            }
            SN_REPSAL => {
                read_rep_section(&mut r, &mut a, len, &mut repsal, &mut repsal_first)?;
            }
            SN_NOBREAK => {
                nobreak = true;
                r.skip(len)?;
            }
            SN_WORDS => {
                read_words_section(&mut r, &mut a, len, &mut common_words)?;
            }
            _ => {
                if flags & SNF_REQUIRED != 0 {
                    bail!(UnknownRequiredSection)
                }
                r.skip(len)?;
            }
        }
    }

    let foldtree = read_wordtree(&mut r, false)?;
    let keeptree = read_wordtree(&mut r, false)?;
    let prefixtree = read_wordtree(&mut r, true)?;

    if !midword.is_empty() {
        charflags.apply_midword(&a[midword]);
    }

    Ok(Dictionary {
        hasher: hashbrown::DefaultHashBuilder::default(),
        user_banned_words: HashTable::new(),
        user_good_words: HashTable::new(),
        arena: a,
        foldtree,
        keeptree,
        prefixtree,
        charflags,
        regions,
        region: REGION_ALL,
        prefcond,
        comp_max,
        comp_minlen,
        comp_sylmax,
        comp_options,
        comp_rules,
        comp_patterns,
        syllable,
        nobreak,
        sal,
        map,
        rep,
        rep_first,
        repsal,
        repsal_first,
        common_words,
    })
}

fn read_charflags(r: &mut SpellReader, len: usize, cf: &mut CharFlags) -> Result<(), ParseError> {
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

fn read_prefcond(r: &mut SpellReader, a: &mut Arena, len: usize) -> Result<Vec<Bytes>, ParseError> {
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
            a.alloc(data)
        } else {
            Bytes::default()
        };
        conditions.push(cond);
    }

    let remaining = len.saturating_sub(bytes_read);
    if remaining > 0 {
        r.skip(remaining)?;
    }

    Ok(conditions)
}

#[allow(clippy::too_many_arguments)]
fn read_compound(
    r: &mut SpellReader,
    a: &mut Arena,
    len: usize,
    comp_max: &mut u8,
    comp_minlen: &mut u8,
    comp_sylmax: &mut u8,
    comp_options: &mut u8,
    comp_rules: &mut CompoundRules,
    comp_patterns: &mut Vec<(Bytes, Bytes)>,
) -> Result<(), ParseError> {
    if len < 2 {
        r.skip(len)?;
        return Ok(());
    }

    let mut todo = len;

    let c = r.read_u8()?;
    todo -= 1;
    *comp_max = if c < 2 { MAXWLEN as u8 } else { c };

    let c = r.read_u8()?;
    todo -= 1;
    *comp_minlen = if c < 1 { 0 } else { c };

    if todo == 0 {
        return Ok(());
    }

    let c = r.read_u8()?;
    todo -= 1;
    *comp_sylmax = if c < 1 { MAXWLEN as u8 } else { c };

    if todo == 0 {
        return Ok(());
    }

    let first = r.read_u8()?;
    if first != 0 {
        todo -= 1;
        comp_rules.all_flags.push(first);
        comp_rules.start_flags.push(first);
    } else {
        todo -= 1;
        if todo == 0 {
            return Ok(());
        }
        let opts = r.read_u8()?;
        todo -= 1;
        *comp_options = opts;

        if todo < 2 {
            r.skip(todo)?;
            return Ok(());
        }

        let pat_count = r.read_u16_be()? as usize;
        todo -= 2;

        for _ in 0..pat_count {
            if todo == 0 {
                break;
            }
            let pat_len = r.read_u8()? as usize;
            todo -= 1;
            if pat_len == 0 || todo < pat_len {
                r.skip(todo)?;
                return Ok(());
            }
            let pat = r.read_exact(pat_len)?;
            todo -= pat_len;

            let split_pos = pat.iter().position(|&b| b == b'/');
            let (first_part, second_part) = match split_pos {
                Some(pos) => (&pat[..pos], &pat[pos + 1..]),
                None => (pat, &[][..]),
            };
            comp_patterns.push((a.alloc(first_part), a.alloc(second_part)));
        }
    }

    if todo == 0 {
        return Ok(());
    }

    let mut at_start = true;
    let mut in_bracket = false;
    let mut current_rule = Vec::new();

    let flags_data = r.read_exact(todo)?;
    for &c in flags_data {
        let is_special = matches!(c, b'?' | b'*' | b'+' | b'[' | b']' | b'/');

        if !is_special {
            if !comp_rules.all_flags.contains(&c) {
                comp_rules.all_flags.push(c);
            }

            if (at_start || in_bracket) && !comp_rules.start_flags.contains(&c) {
                comp_rules.start_flags.push(c);
            }

            if at_start && !in_bracket {
                at_start = false;
            }
        }

        match c {
            b'[' => {
                in_bracket = true;
                current_rule.push(c);
            }
            b']' => {
                in_bracket = false;
                at_start = false;
                current_rule.push(c);
            }
            b'/' => {
                if !current_rule.is_empty() {
                    comp_rules.rules.push(a.alloc(&current_rule));
                    current_rule.clear();
                }
                at_start = true;
            }
            _ => {
                current_rule.push(c);
            }
        }
    }

    if !current_rule.is_empty() {
        comp_rules.rules.push(a.alloc(&current_rule));
    }

    Ok(())
}

fn read_syllable(
    r: &mut SpellReader,
    a: &mut Arena,
    len: usize,
    syllable: &mut Syllable,
) -> Result<(), ParseError> {
    if len == 0 {
        return Ok(());
    }

    let data = r.read_exact(len)?;
    let mut parts = data.split(|&b| b == b'/');

    if let Some(chars) = parts.next() {
        syllable.chars = a.alloc(chars);
    }

    for part in parts {
        if !part.is_empty() {
            syllable.items.push(SyllableItem {
                chars: a.alloc(part),
            });
        }
    }

    Ok(())
}

fn read_rep_section(
    r: &mut SpellReader,
    a: &mut Arena,
    len: usize,
    items: &mut Vec<RepItem>,
    first: &mut [i16; 256],
) -> Result<(), ParseError> {
    if len < 2 {
        r.skip(len)?;
        return Ok(());
    }

    let count = r.read_u16_be()? as usize;

    items.reserve(count);
    for _ in 0..count {
        let from_len = r.read_u8()? as usize;
        let from_data = r.read_exact(from_len)?;
        let to_len = r.read_u8()? as usize;
        let to_data = r.read_exact(to_len)?;
        items.push(RepItem {
            from: a.alloc(from_data),
            to: a.alloc(to_data),
        });
    }

    // probably don't need this becaus parent initialized correctly
    first.fill(-1);

    for (i, item) in items.iter().enumerate() {
        let b = a[item.from][0] as usize;
        if first[b] == -1 {
            first[b] = i as i16;
        }
    }

    Ok(())
}

fn read_words_section(
    r: &mut SpellReader,
    a: &mut Arena,
    len: usize,
    words: &mut CommonWords,
) -> Result<(), ParseError> {
    if len == 0 {
        return Ok(());
    }
    let data = r.read_exact(len)?;

    let word_count = data.iter().filter(|&&b| b == 0).count();
    *words = CommonWords::with_capacity(word_count);

    let mut start = 0;
    for i in 0..data.len() {
        if data[i] != 0 {
            continue;
        }
        let word = &data[start..i];
        start = i + 1;
        if word.is_empty() || word.len() > MAXWLEN {
            continue;
        }
        let word = a.alloc(word);
        words.insert(a, word, 10);
    }

    Ok(())
}

fn read_map_section(r: &mut SpellReader, len: usize) -> Result<MapInfo, ParseError> {
    if len == 0 {
        return Ok(MapInfo {
            map_array: [0; 256],
        });
    }

    let data = r.read_exact(len)?;
    let Ok(map_str) = std::str::from_utf8(data) else {
        bail!(UnknownRequiredSection)
    };

    let mut map_array = [0u32; 256];
    let mut head: u32 = 0;

    for c in map_str.chars() {
        if c == '/' {
            head = 0;
            continue;
        }
        if head == 0 {
            head = c as u32;
        }
        let code = c as u32;
        if code < 256 {
            map_array[code as usize] = head;
        }
    }

    Ok(MapInfo { map_array })
}

fn read_sal_section(r: &mut SpellReader, len: usize) -> Result<SalInfo, ParseError> {
    if len < 3 {
        bail!(UnexpectedEof)
    }

    let salflags = r.read_u8()?;
    let followup = salflags & SAL_F0LLOWUP != 0;
    let collapse = salflags & SAL_COLLAPSE != 0;
    let rem_accents = salflags & SAL_REM_ACCENTS != 0;

    let salcount = r.read_u16_be()? as usize;

    let mut items = Vec::with_capacity(salcount + 1);

    for _ in 0..salcount {
        let fromlen = r.read_u8()? as usize;
        let from = r.read_exact(fromlen)?;
        let tolen = r.read_u8()? as usize;
        let to_bytes = r.read_exact(tolen)?;

        // Parse the "from" pattern into lead, oneof, rules.
        let mut lead = Vec::new();
        let mut oneof = Vec::new();
        let mut rules = Vec::new();

        let Ok(as_str) = std::str::from_utf8(from) else {
            bail!(UnknownRequiredSection)
        };
        let from_chars: Vec<char> = as_str.chars().collect();

        let mut fi = 0;

        // Read lead: chars until a special ASCII char.
        while fi < from_chars.len() {
            let c = from_chars[fi];
            if c.is_ascii() && b"0123456789(-<^$".contains(&(c as u8)) {
                break;
            }
            lead.push(c);
            fi += 1;
        }

        // Check for (abc) oneof group.
        if fi < from_chars.len() && from_chars[fi] == '(' {
            fi += 1; // skip '('
            while fi < from_chars.len() && from_chars[fi] != ')' {
                oneof.push(from_chars[fi]);
                fi += 1;
            }
            if fi < from_chars.len() {
                fi += 1; // skip ')'
            }
        }

        // Everything remaining goes into rules (as bytes, they're ASCII).
        for &ch in &from_chars[fi..] {
            rules.push(ch as u8);
        }

        let Ok(to_str) = std::str::from_utf8(to_bytes) else {
            bail!(UnexpectedEof)
        };
        let to: Vec<char> = to_str.chars().collect();

        items.push(SalItem {
            lead,
            oneof,
            rules,
            to,
        });
    }

    items.sort_by_key(|item| {
        if let Some(&first) = item.lead.first() {
            (first as u32 & 0xff) as u16
        } else {
            256u16 // empty lead goes to end
        }
    });

    items.push(SalItem {
        lead: Vec::new(),
        oneof: Vec::new(),
        rules: Vec::new(),
        to: Vec::new(),
    });

    let mut first = [-1i32; 256];
    for (i, item) in items.iter().enumerate() {
        if item.lead.is_empty() {
            break; // sentinel
        }
        let c = (item.lead[0] as u32 & 0xff) as usize;
        if first[c] == -1 {
            first[c] = i as i32;
        }
    }

    Ok(SalInfo {
        items,
        first,
        followup,
        collapse,
        rem_accents,
    })
}

fn read_wordtree(r: &mut SpellReader, prefixtree: bool) -> Result<WordTree, ParseError> {
    let node_count = r.read_u32_be()? as usize;

    if node_count == 0 {
        return Ok(WordTree::new());
    }

    let mut node = vec![0u8; node_count];
    let mut meta = vec![0u32; node_count];
    if prefixtree {
        read_tree_node_prefixtree(r, &mut node, &mut meta, node_count, 0)?;
    } else {
        read_tree_node(r, &mut node, &mut meta, node_count, 0)?;
    }

    Ok(WordTree {
        node: node,
        meta: meta,
    })
}

const SHARED_MASK: u32 = 0x8000000;

macro_rules! rtry {
    ($($tt:tt)*) => {
        match $($tt)* {
            Ok(v) => v,
            Err(e) => return Err(e),
        }
    };
}

fn read_tree_node(
    r: &mut SpellReader,
    node: &mut [u8],
    meta: &mut [u32],
    maxidx: usize,
    startidx: usize,
) -> Result<usize, ParseError> {
    let mut idx = startidx;

    let len = rtry!(r.read_u8()) as usize;
    if len == 0 {
        bail!(InvalidSiblingCount)
    }

    if startidx + len >= maxidx {
        bail!(TreeIndexOverflow)
    }

    node[idx] = len as u8;
    // len == 1 represents the majority of nodes,
    // special-casing this reduces instruction counts by 15%
    if len == 1 {
        idx += 1;
        let ch = rtry!(r.read_u8());
        if ch > BY_SPECIAL {
            node[idx] = ch;
            meta[idx] = idx as u32 + 1;
            return read_tree_node(r, node, meta, maxidx, idx + 1);
        } else if ch == BY_INDEX {
            let n = rtry!(r.read_u32_be());
            node[idx] = n as u8;
            let n = n >> 8;
            if n as usize >= maxidx {
                bail!(InvalidSharedIndex)
            }
            meta[idx] = n;
        } else if ch == BY_NOFLAGS {
            node[idx] = 0;
        } else if ch == BY_FLAGS {
            let mut flags = rtry!(r.read_u8()) as u32;
            if flags & (WF_REGION as u32) != 0 {
                flags |= (rtry!(r.read_u8()) as u32) << 16;
            }
            if flags & (WF_AFX as u32) != 0 {
                flags |= (rtry!(r.read_u8()) as u32) << 24;
            }
            meta[idx] = flags;
        } else {
            let mut flags = r.read_u16_le()? as u32;
            if flags & (WF_REGION as u32) != 0 {
                flags |= (rtry!(r.read_u8()) as u32) << 16;
            }
            if flags & (WF_AFX as u32) != 0 {
                flags |= (rtry!(r.read_u8()) as u32) << 24;
            }
            meta[idx] = flags;
        }
        return Ok(idx + 1);
    }
    for _ in 0..len {
        idx += 1;
        let ch = rtry!(r.read_u8());
        if ch > BY_SPECIAL {
            node[idx] = ch;
        } else if ch == BY_INDEX {
            let n = rtry!(r.read_u32_be());
            node[idx] = n as u8;
            let n = n >> 8;
            if n as usize >= maxidx {
                bail!(InvalidSharedIndex)
            }
            meta[idx] = n | SHARED_MASK;
        } else if ch == BY_NOFLAGS {
            node[idx] = 0;
        } else if ch == BY_FLAGS {
            let mut flags = rtry!(r.read_u8()) as u32;
            if flags & (WF_REGION as u32) != 0 {
                flags |= (rtry!(r.read_u8()) as u32) << 16;
            }
            if flags & (WF_AFX as u32) != 0 {
                flags |= (rtry!(r.read_u8()) as u32) << 24;
            }
            meta[idx] = flags;
        } else {
            let mut flags = r.read_u16_le()? as u32;
            if flags & (WF_REGION as u32) != 0 {
                flags |= (rtry!(r.read_u8()) as u32) << 16;
            }
            if flags & (WF_AFX as u32) != 0 {
                flags |= (rtry!(r.read_u8()) as u32) << 24;
            }
            meta[idx] = flags;
        }
    }
    idx += 1;
    for i in 1..len + 1 {
        let pos = startidx + i;
        if node[pos] != 0 {
            if meta[pos] & SHARED_MASK != 0 {
                meta[pos] &= !SHARED_MASK;
            } else {
                meta[pos] = idx as u32;
                idx = rtry!(read_tree_node(r, node, meta, maxidx, idx));
            }
        }
    }
    Ok(idx)
}

fn read_tree_node_prefixtree(
    r: &mut SpellReader,
    node: &mut [u8],
    meta: &mut [u32],
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

    node[idx] = len as u8;
    idx += 1;

    for _ in 0..len {
        let c = r.read_u8()?;

        if c <= BY_SPECIAL {
            if c == BY_INDEX {
                let n = r.read_u24_be()?;
                if n as usize >= maxidx {
                    bail!(InvalidSharedIndex)
                }
                meta[idx] = n | SHARED_MASK;
                let xbyte = r.read_u8()?;
                node[idx] = xbyte;
            } else {
                let pflags = if c == BY_FLAGS {
                    (r.read_u8()? as u32) << 24
                } else {
                    0
                };
                let affix_id = r.read_u8()? as u32;
                let prefcondnr = r.read_u16_be()? as u32;
                meta[idx] = pflags | (prefcondnr << 8) | affix_id;
            }
        } else {
            node[idx] = c;
        }
        idx += 1;
    }

    for i in 1..len + 1 {
        let pos = startidx + i;
        if node[pos] != 0 {
            if meta[pos] & SHARED_MASK != 0 {
                meta[pos] &= !SHARED_MASK;
            } else {
                meta[pos] = idx as u32;
                idx = read_tree_node_prefixtree(r, node, meta, maxidx, idx)?;
            }
        }
    }

    Ok(idx)
}
