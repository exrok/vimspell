use super::CharFlags;
use super::MAXWLEN;
use super::SCORE_DEL;
use super::SCORE_INS;
use super::SCORE_MAXMAX;
use super::SCORE_SUBST;
use super::SCORE_SWAP;
use super::SalInfo;

pub(crate) fn is_word_char_w(c: char, charflags: &CharFlags) -> bool {
    if (c as u32) < 256 {
        charflags.is_word_char(c as u8)
    } else {
        true
    }
}

/// Port of spell_soundfold_wsal from Neovim's spell.c:2891-3181.
/// Applies SAL phonetic rules to produce a soundfolded representation.
pub(crate) fn soundfold_wsal(sal: &SalInfo, input: &[u8], charflags: &CharFlags) -> Vec<u8> {
    let as_str = std::str::from_utf8(input).unwrap_or("");
    let mut word: Vec<char> = Vec::with_capacity(MAXWLEN + 1);
    let mut did_white = false;
    for ch in as_str.chars() {
        if sal.rem_accents {
            if ch == ' ' || ch == '\t' || ch == '\u{a0}' {
                if did_white {
                    continue;
                }
                word.push(' ');
                did_white = true;
                continue;
            }
            did_white = false;
            if !is_word_char_w(ch, charflags) {
                continue;
            }
        }
        word.push(ch);
        if word.len() >= MAXWLEN - 1 {
            break;
        }
    }
    word.push('\0'); // NUL sentinel

    let low_byte = |c: char| (c as u32 & 0xff) as usize;

    let smp = &sal.items;
    let mut wres: Vec<char> = Vec::with_capacity(MAXWLEN);
    let mut k: usize = 0;
    let mut p0: i32 = -333;
    let mut i: usize = 0;
    let mut z: bool = false;

    while word[i] != '\0' {
        let mut c = word[i];
        let n_start = sal.first[low_byte(c)];
        let mut z0 = false;

        if n_start >= 0 {
            let mut n = n_start as usize;

            while n < smp.len()
                && !smp[n].lead.is_empty()
                && low_byte(smp[n].lead[0]) == low_byte(c)
            {
                if c != smp[n].lead[0] {
                    n += 1;
                    continue;
                }
                k = smp[n].lead.len();
                if k > 1 {
                    if word[i + 1] != smp[n].lead[1] {
                        n += 1;
                        continue;
                    }
                    if k > 2 {
                        let mut matched = true;
                        for j in 2..k {
                            if word[i + j] != smp[n].lead[j] {
                                matched = false;
                                break;
                            }
                        }
                        if !matched {
                            n += 1;
                            continue;
                        }
                    }
                }

                if !smp[n].oneof.is_empty() {
                    if !smp[n].oneof.contains(&word[i + k]) {
                        n += 1;
                        continue;
                    }
                    k += 1;
                }

                let rules = &smp[n].rules;
                let mut pri: i32 = 5;

                p0 = rules.first().map_or(0, |&b| b as i32);
                let k0 = k;
                let mut si = 0usize;

                while rules.get(si) == Some(&b'-') && k > 1 {
                    k -= 1;
                    si += 1;
                }
                if rules.get(si) == Some(&b'<') {
                    si += 1;
                }
                if let Some(&b) = rules.get(si)
                    && b.is_ascii_digit()
                {
                    pri = (b - b'0') as i32;
                    si += 1;
                }
                if rules.get(si..).is_some_and(|r| r.starts_with(b"^^")) {
                    si += 1;
                }

                let sc = rules.get(si).copied().unwrap_or(0);
                let sc_next = rules.get(si + 1).copied().unwrap_or(0);

                let wk0 = word[i + k0];
                let is_word_at_k0 = wk0 != '\0' && is_word_char_w(wk0, charflags);

                let prev_is_word =
                    i > 0 && (word[i - 1] == ' ' || is_word_char_w(word[i - 1], charflags));

                let cond_ok = sc == 0
                    || (sc == b'^'
                        && (i == 0 || !prev_is_word)
                        && (sc_next != b'$' || !is_word_at_k0))
                    || (sc == b'$' && i > 0 && prev_is_word && !is_word_at_k0);

                if !cond_ok {
                    n += 1;
                    continue;
                }

                // Search for followup rules.
                let c0 = word[i + k - 1];
                let n0_start = sal.first[low_byte(c0)];

                let mut followup_wins = false;
                if sal.followup
                    && k > 1
                    && n0_start >= 0
                    && p0 != b'-' as i32
                    && word[i + k] != '\0'
                {
                    let mut n0 = n0_start as usize;
                    let mut found_followup = false;

                    while n0 < smp.len()
                        && !smp[n0].lead.is_empty()
                        && low_byte(smp[n0].lead[0]) == low_byte(c0)
                    {
                        if c0 != smp[n0].lead[0] {
                            n0 += 1;
                            continue;
                        }
                        let mut fk0 = smp[n0].lead.len();
                        if fk0 > 1 {
                            if word[i + k] != smp[n0].lead[1] {
                                n0 += 1;
                                continue;
                            }
                            if fk0 > 2 {
                                let mut matched = true;
                                for j in 2..fk0 {
                                    if word[i + k + j - 1] != smp[n0].lead[j] {
                                        matched = false;
                                        break;
                                    }
                                }
                                if !matched {
                                    n0 += 1;
                                    continue;
                                }
                            }
                        }
                        fk0 += k - 1;

                        if !smp[n0].oneof.is_empty() {
                            if !smp[n0].oneof.contains(&word[i + fk0]) {
                                n0 += 1;
                                continue;
                            }
                            fk0 += 1;
                        }

                        let mut fp0: i32 = 5;
                        let frules = &smp[n0].rules;
                        let mut fsi = 0usize;
                        while frules.get(fsi) == Some(&b'-') {
                            fsi += 1;
                        }
                        if frules.get(fsi) == Some(&b'<') {
                            fsi += 1;
                        }
                        if let Some(&b) = frules.get(fsi)
                            && b.is_ascii_digit()
                        {
                            fp0 = (b - b'0') as i32;
                            fsi += 1;
                        }

                        let fcond = frules.get(fsi).copied().unwrap_or(0);
                        let fwk0 = word[i + fk0];
                        let f_is_word_at_k0 = fwk0 != '\0' && is_word_char_w(fwk0, charflags);

                        if fcond == 0 || (fcond == b'$' && !f_is_word_at_k0) {
                            if fk0 == k {
                                n0 += 1;
                                continue;
                            }
                            if fp0 < pri {
                                n0 += 1;
                                continue;
                            }
                            found_followup = true;
                            break;
                        }
                        n0 += 1;
                    }

                    if found_followup
                        && n0 < smp.len()
                        && !smp[n0].lead.is_empty()
                        && low_byte(smp[n0].lead[0]) == low_byte(c0)
                    {
                        followup_wins = true;
                    }
                }

                if followup_wins {
                    n += 1;
                    continue;
                }

                // Apply replacement.
                let to = &smp[n].to;
                let rules_ref = &smp[n].rules;
                let has_lt = rules_ref.contains(&b'<');
                p0 = if has_lt { 1 } else { 0 };

                if has_lt && !z {
                    // In-place replacement ('<').
                    if !wres.is_empty()
                        && !to.is_empty()
                        && (*wres.last().unwrap() == c || *wres.last().unwrap() == to[0])
                    {
                        wres.pop();
                    }
                    z0 = true;
                    z = true;
                    let mut k0_ip = 0usize;
                    for &tc in to {
                        if word[i + k0_ip] == '\0' {
                            break;
                        }
                        word[i + k0_ip] = tc;
                        k0_ip += 1;
                    }
                    if k > k0_ip {
                        let start = i + k0_ip;
                        let end = i + k;
                        if end <= word.len() {
                            word.drain(start..end);
                        }
                    }
                    c = word[i];
                } else {
                    // Normal replacement.
                    i += k - 1;
                    z = false;
                    if !to.is_empty() {
                        for c in &to[0..to.len() - 1] {
                            if wres.len() >= MAXWLEN {
                                break;
                            }
                            if wres.is_empty() || *wres.last().unwrap() != *c {
                                wres.push(*c);
                            }
                        }
                    }
                    c = if to.is_empty() {
                        '\0'
                    } else {
                        *to.last().unwrap()
                    };

                    if rules.windows(2).any(|w| w[0] == b'^' && w[1] == b'^') {
                        if c != '\0' && wres.len() < MAXWLEN {
                            wres.push(c);
                        }
                        let shift = i + 1;
                        if shift < word.len() {
                            word.drain(0..shift);
                        }
                        i = 0;
                        z0 = true;
                    }
                }
                break;
            }
        } else if c == ' ' || c == '\t' {
            c = ' ';
            k = 1;
        }

        // Output section.
        if !z0 {
            if k != 0
                && p0 == 0
                && wres.len() < MAXWLEN
                && c != '\0'
                && (!sal.collapse || wres.is_empty() || *wres.last().unwrap() != c)
            {
                wres.push(c);
            }
            i += 1;
            z = false;
            k = 0;
        }
    }

    let result: String = wres.into_iter().collect();
    result.into_bytes()
}

/// Port of soundalike_score from Neovim's spellsuggest.c:3247-3456.
/// Compare two soundfolded strings and return a score (lower = more similar).
pub(crate) fn soundalike_score(goodstart: &[u8], badstart: &[u8]) -> i32 {
    let mut goodsound = goodstart;
    let mut badsound = badstart;
    let mut score = 0i32;

    // Handle '*' (vowel) at start.
    if (!badsound.is_empty() || !goodsound.is_empty())
        && ((badsound.first() == Some(&b'*') || goodsound.first() == Some(&b'*'))
            && badsound.first() != goodsound.first())
    {
        if (badsound.is_empty() && goodsound.len() == 2)
            || (goodsound.is_empty() && badsound.len() == 2)
        {
            return SCORE_DEL;
        }
        if badsound.is_empty() || goodsound.is_empty() {
            return SCORE_MAXMAX;
        }

        if badsound
            .get(1)
            .zip(goodsound.get(1))
            .is_some_and(|(a, b)| a == b)
            || badsound
                .get(2)
                .zip(goodsound.get(2))
                .is_some_and(|(a, b)| a == b)
        {
            // Handle like a substitute.
        } else {
            score = 2 * SCORE_DEL / 3;
            if badsound.first() == Some(&b'*') {
                badsound = &badsound[1..];
            } else {
                goodsound = &goodsound[1..];
            }
        }
    }

    let goodlen = goodsound.len() as i32;
    let badlen = badsound.len() as i32;

    let n = goodlen - badlen;
    if !(-2..=2).contains(&n) {
        return SCORE_MAXMAX;
    }

    // pl = longest, ps = shortest.
    let (mut pl, mut ps) = if n > 0 {
        (goodsound, badsound)
    } else {
        (badsound, goodsound)
    };

    // Skip identical prefix.
    while pl.first().is_some_and(|a| ps.first() == Some(a)) {
        pl = &pl[1..];
        ps = &ps[1..];
    }

    match n {
        -2 | 2 => {
            // Must delete two characters from pl.
            if pl.is_empty() {
                return SCORE_MAXMAX;
            }
            pl = &pl[1..]; // first delete
            while pl.first().is_some_and(|a| ps.first() == Some(a)) {
                pl = &pl[1..];
                ps = &ps[1..];
            }
            if let [_, rest @ ..] = pl
                && rest == ps
            {
                return score + SCORE_DEL * 2;
            }
        }
        -1 | 1 => {
            // At least one delete from pl.

            // 1: delete
            let (mut pl2, mut ps2) = (&pl[1..], ps);
            while pl2.first().is_some_and(|a| ps2.first() == Some(a)) {
                pl2 = &pl2[1..];
                ps2 = &ps2[1..];
            }
            if pl2.is_empty() && ps2.is_empty() {
                return score + SCORE_DEL;
            }

            // 2: delete then swap
            if let [a1, b1, rest1 @ ..] = pl2
                && let [a2, b2, rest2 @ ..] = ps2
                && a1 == b2
                && b1 == a2
                && rest1 == rest2
            {
                return score + SCORE_DEL + SCORE_SWAP;
            }

            // 3: delete then substitute
            if let [_, rest_a @ ..] = pl2
                && let [_, rest_b @ ..] = ps2
                && rest_a == rest_b
            {
                return score + SCORE_DEL + SCORE_SUBST;
            }

            // 4: first swap then delete
            if let [a, b, rest_pl @ ..] = pl
                && let [c, d, rest_ps @ ..] = ps
                && a == d
                && b == c
            {
                let (mut pl2, mut ps2) = (rest_pl, rest_ps);
                while pl2.first().is_some_and(|a| ps2.first() == Some(a)) {
                    pl2 = &pl2[1..];
                    ps2 = &ps2[1..];
                }
                if let [_, rest @ ..] = pl2
                    && rest == ps2
                {
                    return score + SCORE_SWAP + SCORE_DEL;
                }
            }

            // 5: first substitute then delete
            if let [_, rest_pl @ ..] = pl
                && let [_, rest_ps @ ..] = ps
            {
                let (mut pl2, mut ps2) = (rest_pl, rest_ps);
                while pl2.first().is_some_and(|a| ps2.first() == Some(a)) {
                    pl2 = &pl2[1..];
                    ps2 = &ps2[1..];
                }
                if let [_, rest @ ..] = pl2
                    && rest == ps2
                {
                    return score + SCORE_SUBST + SCORE_DEL;
                }
            }
        }
        0 => {
            // Same length.
            // 1: identical
            if pl.is_empty() {
                return score;
            }

            // 2: swap
            if let [a, b, rest_pl @ ..] = pl
                && let [c, d, rest_ps @ ..] = ps
                && a == d
                && b == c
            {
                let (mut pl2, mut ps2) = (rest_pl, rest_ps);
                while pl2.first().is_some_and(|a| ps2.first() == Some(a)) {
                    pl2 = &pl2[1..];
                    ps2 = &ps2[1..];
                }
                if pl2.is_empty() && ps2.is_empty() {
                    return score + SCORE_SWAP;
                }
                // 3: swap and swap
                if let [a1, b1, rest1 @ ..] = pl2
                    && let [a2, b2, rest2 @ ..] = ps2
                    && a1 == b2
                    && b1 == a2
                    && rest1 == rest2
                {
                    return score + SCORE_SWAP + SCORE_SWAP;
                }
                // 4: swap and substitute
                if let [_, rest_a @ ..] = pl2
                    && let [_, rest_b @ ..] = ps2
                    && rest_a == rest_b
                {
                    return score + SCORE_SWAP + SCORE_SUBST;
                }
            }

            // 5: substitute
            if let [_, rest_pl @ ..] = pl
                && let [_, rest_ps @ ..] = ps
            {
                let (mut pl2, mut ps2) = (rest_pl, rest_ps);
                while pl2.first().is_some_and(|a| ps2.first() == Some(a)) {
                    pl2 = &pl2[1..];
                    ps2 = &ps2[1..];
                }
                if pl2.is_empty() && ps2.is_empty() {
                    return score + SCORE_SUBST;
                }
                // 6: substitute and swap
                if let [a1, b1, rest1 @ ..] = pl2
                    && let [a2, b2, rest2 @ ..] = ps2
                    && a1 == b2
                    && b1 == a2
                    && rest1 == rest2
                {
                    return score + SCORE_SUBST + SCORE_SWAP;
                }
                // 7: substitute and substitute
                if let [_, rest_a @ ..] = pl2
                    && let [_, rest_b @ ..] = ps2
                    && rest_a == rest_b
                {
                    return score + SCORE_SUBST + SCORE_SUBST;
                }
                // 8: insert then delete
                let (mut pl3, mut ps3) = (pl, rest_ps);
                while pl3.first().is_some_and(|a| ps3.first() == Some(a)) {
                    pl3 = &pl3[1..];
                    ps3 = &ps3[1..];
                }
                if let [_, rest @ ..] = pl3
                    && !ps3.is_empty()
                    && rest == ps3
                {
                    return score + SCORE_INS + SCORE_DEL;
                }
                // 9: delete then insert
                let (mut pl3, mut ps3) = (rest_pl, ps);
                while pl3.first().is_some_and(|a| ps3.first() == Some(a)) {
                    pl3 = &pl3[1..];
                    ps3 = &ps3[1..];
                }
                if let [_, rest @ ..] = ps3
                    && !pl3.is_empty()
                    && pl3 == rest
                {
                    return score + SCORE_INS + SCORE_DEL;
                }
            }
        }
        _ => {}
    }

    SCORE_MAXMAX
}
