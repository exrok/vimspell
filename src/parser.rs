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
            SN_NOBREAK => {
                nobreak = true;
                r.skip(len)?;
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

    Ok(Dictionary {
        arena: a,
        foldtree,
        keeptree,
        prefixtree,
        charflags,
        regions,
        midword,
        prefcond,
        comp_max,
        comp_minlen,
        comp_sylmax,
        comp_options,
        comp_rules,
        comp_patterns,
        syllable,
        nobreak,
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

            if at_start || in_bracket {
                if !comp_rules.start_flags.contains(&c) {
                    comp_rules.start_flags.push(c);
                }
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

fn read_wordtree(r: &mut SpellReader, prefixtree: bool) -> Result<WordTree, ParseError> {
    let node_count = r.read_u32_be()? as usize;

    if node_count == 0 {
        return Ok(WordTree::new());
    }

    let mut byts = vec![0u8; node_count];
    let mut idxs = vec![0u32; node_count];

    read_tree_node(r, &mut byts, &mut idxs, node_count, 0, prefixtree)?;

    Ok(WordTree { byts, idxs })
}

const SHARED_MASK: u32 = 0x8000000;

fn read_tree_node(
    r: &mut SpellReader,
    byts: &mut [u8],
    idxs: &mut [u32],
    maxidx: usize,
    startidx: usize,
    prefixtree: bool,
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
            if c == BY_NOFLAGS && !prefixtree {
                idxs[idx] = 0;
                byts[idx] = 0;
            } else if c == BY_INDEX {
                let n = r.read_u24_be()?;
                if n as usize >= maxidx {
                    bail!(InvalidSharedIndex)
                }
                idxs[idx] = n | SHARED_MASK;
                let xbyte = r.read_u8()?;
                byts[idx] = xbyte;
            } else if prefixtree {
                let pflags = if c == BY_FLAGS {
                    (r.read_u8()? as u32) << 24
                } else {
                    0
                };
                let affix_id = r.read_u8()? as u32;
                let prefcondnr = r.read_u16_be()? as u32;
                idxs[idx] = pflags | (prefcondnr << 8) | affix_id;
                byts[idx] = 0;
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
            if idxs[pos] & SHARED_MASK != 0 {
                idxs[pos] &= !SHARED_MASK;
            } else {
                idxs[pos] = idx as u32;
                idx = read_tree_node(r, byts, idxs, maxidx, idx, prefixtree)?;
            }
        }
    }

    Ok(idx)
}
