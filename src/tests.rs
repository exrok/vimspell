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

#[test]
fn test_compound_rules_simple() {
    let mut arena = Arena::default();
    let mut rules = CompoundRules::new();
    rules.rules.push(arena.alloc(b"abc"));
    rules.start_flags.push(b'a');
    rules.all_flags.extend_from_slice(&[b'a', b'b', b'c']);

    assert!(rules.flag_allowed_at_start(b'a'));
    assert!(!rules.flag_allowed_at_start(b'b'));
    assert!(rules.flag_allowed(b'a'));
    assert!(rules.flag_allowed(b'b'));
    assert!(rules.flag_allowed(b'c'));

    assert!(rules.matches_partial(&arena, &[b'a']));
    assert!(rules.matches_partial(&arena, &[b'a', b'b']));
    assert!(rules.matches_partial(&arena, &[b'a', b'b', b'c']));
    assert!(!rules.matches_partial(&arena, &[b'x']));

    assert!(rules.matches_complete(&arena, &[b'a', b'b', b'c']));
    assert!(!rules.matches_complete(&arena, &[b'a', b'b']));
    assert!(!rules.matches_complete(&arena, &[b'a', b'b', b'c', b'd']));
}

#[test]
fn test_compound_rules_with_brackets() {
    let mut arena = Arena::default();
    let mut rules = CompoundRules::new();
    rules.rules.push(arena.alloc(b"[ab]c"));
    rules.start_flags.extend_from_slice(&[b'a', b'b']);
    rules.all_flags.extend_from_slice(&[b'a', b'b', b'c']);

    assert!(rules.matches_partial(&arena, &[b'a']));
    assert!(rules.matches_partial(&arena, &[b'b']));
    assert!(rules.matches_partial(&arena, &[b'a', b'c']));
    assert!(rules.matches_partial(&arena, &[b'b', b'c']));

    assert!(rules.matches_complete(&arena, &[b'a', b'c']));
    assert!(rules.matches_complete(&arena, &[b'b', b'c']));
    assert!(!rules.matches_complete(&arena, &[b'a']));
    assert!(!rules.matches_complete(&arena, &[b'c', b'a']));
}

#[test]
fn test_compound_rules_with_plus() {
    let mut arena = Arena::default();
    let mut rules = CompoundRules::new();
    rules.rules.push(arena.alloc(b"a+b"));
    rules.start_flags.push(b'a');
    rules.all_flags.extend_from_slice(&[b'a', b'b']);

    assert!(rules.matches_complete(&arena, &[b'a', b'b']));
    assert!(!rules.matches_complete(&arena, &[b'b']));
}

#[test]
fn test_compound_rules_multiple() {
    let mut arena = Arena::default();
    let mut rules = CompoundRules::new();
    rules.rules.push(arena.alloc(b"ab"));
    rules.rules.push(arena.alloc(b"cd"));
    rules.start_flags.extend_from_slice(&[b'a', b'c']);
    rules.all_flags.extend_from_slice(&[b'a', b'b', b'c', b'd']);

    assert!(rules.matches_complete(&arena, &[b'a', b'b']));
    assert!(rules.matches_complete(&arena, &[b'c', b'd']));
    assert!(!rules.matches_complete(&arena, &[b'a', b'd']));
}

#[test]
fn test_syllable_counting_simple() {
    let mut arena = Arena::default();
    let mut syl = Syllable::new();
    syl.chars = arena.alloc(b"aeiou");

    assert_eq!(syl.count(&arena, b"hello"), 2);
    assert_eq!(syl.count(&arena, b"beautiful"), 3);
    assert_eq!(syl.count(&arena, b"xyz"), 0);
}

#[test]
fn test_syllable_counting_with_items() {
    let mut arena = Arena::default();
    let mut syl = Syllable::new();
    syl.chars = arena.alloc(b"aeiou");
    syl.items.push(SyllableItem {
        chars: arena.alloc(b"ou"),
    });

    assert_eq!(syl.count(&arena, b"sound"), 1);
}

#[test]
fn test_syllable_counting_space_reset() {
    let mut arena = Arena::default();
    let mut syl = Syllable::new();
    syl.chars = arena.alloc(b"aeiou");

    assert_eq!(syl.count(&arena, b"he lo"), 1);
}

#[test]
fn test_compound_info() {
    let dict = load_dict();
    let info = dict.compound_info();

    assert_eq!(info.max_words, 254);
    assert_eq!(info.min_part_len, 0);
}

#[test]
fn test_prefix_words_in_foldtree() {
    let dict = load_dict();
    assert!(dict.check_word(b"undo"));
    assert!(dict.check_word(b"unkind"));
    assert!(dict.check_word(b"unable"));
    assert!(dict.check_word(b"unlike"));
    assert!(dict.check_word(b"rewrite"));
    assert!(dict.check_word(b"reopen"));
    assert!(dict.check_word(b"unhappy"));
    assert!(dict.check_word(b"restart"));
}

#[test]
fn test_prefix_invalid_combos() {
    let dict = load_dict();
    assert!(!dict.check_word(b"unxyzabc"));
    assert!(!dict.check_word(b"rexyzabc"));
}

fn build_prefix_dict() -> Dictionary {
    // Manually construct a Dictionary with a synthetic prefix tree
    // to test the prefix matching logic.
    //
    // Word tree contains "happy" with WF_AFX set and affix ID = 5.
    // Prefix tree contains "un" with affix ID = 5 and no condition.
    // So "unhappy" should be valid via prefix stripping.
    let arena = Arena::default();

    // Build foldtree containing "happy" with flags: WF_AFX | affix_id=5 in bits 24-31
    // The trie for "happy": root -> h -> a -> p -> p -> y -> [end with flags]
    //
    // Trie layout: each node starts with a sibling count byte, then sibling entries.
    // A sibling with byte 0 and flags in idxs is an end-of-word marker.
    // A sibling with byte > 3 and child index in idxs is a character node.
    //
    // We encode: h-a-p-p-y with flags at the leaf.
    let word_flags: u32 = (WF_AFX as u32) | (5u32 << 24);

    // Node layout for "happy":
    // [0] = 1 (1 sibling: 'h')
    // [1] = 'h', idxs[1] = 2 (child at index 2)
    // [2] = 1 (1 sibling: 'a')
    // [3] = 'a', idxs[3] = 4
    // [4] = 1 (1 sibling: 'p')
    // [5] = 'p', idxs[5] = 6
    // [6] = 1 (1 sibling: 'p')
    // [7] = 'p', idxs[7] = 8
    // [8] = 1 (1 sibling: 'y')
    // [9] = 'y', idxs[9] = 10
    // [10] = 1 (1 sibling: end-of-word marker)
    // [11] = 0 (end marker), idxs[11] = word_flags
    let foldtree = WordTree {
        byts: vec![1, b'h', 1, b'a', 1, b'p', 1, b'p', 1, b'y', 1, 0],
        idxs: vec![0, 2, 0, 4, 0, 6, 0, 8, 0, 10, 0, word_flags],
    };

    // Build prefix tree containing "un" with affix_id=5, condnr=0, pflags=0
    // idxs value = (pflags << 24) | (condnr << 8) | affix_id = 0 | 0 | 5 = 5
    // Node layout for "un":
    // [0] = 1 (1 sibling: 'u')
    // [1] = 'u', idxs[1] = 2
    // [2] = 1 (1 sibling: 'n')
    // [3] = 'n', idxs[3] = 4
    // [4] = 1 (1 sibling: end-of-prefix marker)
    // [5] = 0 (end marker), idxs[5] = 5 (affix_id=5, condnr=0, pflags=0)
    let prefix_pidx: u32 = 5; // affix_id=5
    let prefixtree = WordTree {
        byts: vec![1, b'u', 1, b'n', 1, 0],
        idxs: vec![0, 2, 0, 4, 0, prefix_pidx],
    };

    // Empty condition (index 0) - always matches
    let prefcond = vec![Bytes::default()];

    Dictionary {
        arena,
        foldtree,
        keeptree: WordTree::new(),
        prefixtree,
        charflags: CharFlags::new(),
        regions: Vec::new(),
        region: REGION_ALL,

        prefcond,
        comp_max: MAXWLEN as u8,
        comp_minlen: 0,
        comp_sylmax: MAXWLEN as u8,
        comp_options: 0,
        comp_rules: CompoundRules::new(),
        comp_patterns: Vec::new(),
        syllable: Syllable::new(),
        nobreak: false,
        sal: None,
        map: None,
        rep: Vec::new(),
        rep_first: [-1; 256],
        repsal: Vec::new(),
        repsal_first: [-1; 256],
        common_words: CommonWords::new(),
    }
}

#[test]
fn test_prefix_synthetic_valid() {
    let dict = build_prefix_dict();
    assert!(dict.check_word(b"unhappy"));
}

#[test]
fn test_prefix_synthetic_root_valid() {
    let dict = build_prefix_dict();
    assert!(dict.check_word(b"happy"));
}

#[test]
fn test_prefix_synthetic_wrong_prefix() {
    let dict = build_prefix_dict();
    assert!(!dict.check_word(b"rehappy"));
}

#[test]
fn test_prefix_synthetic_nonsense_after_prefix() {
    let dict = build_prefix_dict();
    assert!(!dict.check_word(b"unxyzabc"));
}

#[test]
fn test_match_prefix_condition_empty() {
    assert!(match_prefix_condition(b"", b"anything"));
    assert!(match_prefix_condition(b"", b""));
}

#[test]
fn test_match_prefix_condition_literal() {
    assert!(match_prefix_condition(b"ab", b"abcdef"));
    assert!(!match_prefix_condition(b"ab", b"xbcdef"));
    assert!(!match_prefix_condition(b"ab", b"a"));
}

#[test]
fn test_match_prefix_condition_char_class() {
    assert!(match_prefix_condition(b"[abc]", b"bfoo"));
    assert!(match_prefix_condition(b"[abc]", b"afoo"));
    assert!(!match_prefix_condition(b"[abc]", b"xfoo"));
}

#[test]
fn test_match_prefix_condition_negated_class() {
    assert!(match_prefix_condition(b"[^abc]", b"xfoo"));
    assert!(!match_prefix_condition(b"[^abc]", b"afoo"));
    assert!(!match_prefix_condition(b"[^abc]", b"bfoo"));
}

#[test]
fn test_match_prefix_condition_dot() {
    assert!(match_prefix_condition(b".", b"x"));
    assert!(match_prefix_condition(b".", b"anything"));
    assert!(!match_prefix_condition(b".", b""));
}

#[test]
fn test_match_prefix_condition_complex() {
    assert!(match_prefix_condition(b"[aeiou]b", b"abcd"));
    assert!(!match_prefix_condition(b"[aeiou]b", b"xbcd"));
    assert!(!match_prefix_condition(b"[aeiou]b", b"axcd"));
}

#[test]
fn test_prefix_synthetic_with_condition() {
    let mut arena = Arena::default();

    let word_flags: u32 = (WF_AFX as u32) | (7u32 << 24);
    let foldtree = WordTree {
        byts: vec![2, b'a', b'o', 1, b'k', 1, 0, 1, b'k', 1, 0],
        idxs: vec![0, 3, 7, 0, 5, 0, word_flags, 0, 9, 0, word_flags],
    };

    // Condition "[ao]" means the word after prefix must start with 'a' or 'o'.
    let cond = arena.alloc(b"[ao]");
    let prefcond = vec![cond];

    // Prefix "un" with affix_id=7, condnr=0 (index into prefcond)
    let prefix_pidx: u32 = (0u32 << 8) | 7;
    let prefixtree = WordTree {
        byts: vec![1, b'u', 1, b'n', 1, 0],
        idxs: vec![0, 2, 0, 4, 0, prefix_pidx],
    };

    let dict = Dictionary {
        arena,
        foldtree,
        keeptree: WordTree::new(),
        prefixtree,
        charflags: CharFlags::new(),
        regions: Vec::new(),
        region: REGION_ALL,

        prefcond,
        comp_max: MAXWLEN as u8,
        comp_minlen: 0,
        comp_sylmax: MAXWLEN as u8,
        comp_options: 0,
        comp_rules: CompoundRules::new(),
        comp_patterns: Vec::new(),
        syllable: Syllable::new(),
        nobreak: false,
        sal: None,
        map: None,
        rep: Vec::new(),
        rep_first: [-1; 256],
        repsal: Vec::new(),
        repsal_first: [-1; 256],
        common_words: CommonWords::new(),
    };

    // "unok" -> prefix "un" + "ok", "ok" starts with 'o' which is in [ao] -> valid
    assert!(dict.check_word(b"unok"));
    // "unak" -> prefix "un" + "ak", "ak" starts with 'a' which is in [ao] -> valid
    assert!(dict.check_word(b"unak"));
    // Direct lookups also work
    assert!(dict.check_word(b"ok"));
    assert!(dict.check_word(b"ak"));
}

#[test]
fn test_prefix_synthetic_rare_prefix() {
    let arena = Arena::default();

    let word_flags: u32 = (WF_AFX as u32) | (3u32 << 24);
    let foldtree = WordTree {
        byts: vec![1, b'g', 1, b'o', 1, 0],
        idxs: vec![0, 2, 0, 4, 0, word_flags],
    };

    // Prefix with WFP_RARE flag set
    let prefix_pidx: u32 = (WFP_RARE << 24) | 3;
    let prefixtree = WordTree {
        byts: vec![1, b'a', 1, 0],
        idxs: vec![0, 2, 0, prefix_pidx],
    };

    let dict = Dictionary {
        arena,
        foldtree,
        keeptree: WordTree::new(),
        prefixtree,
        charflags: CharFlags::new(),
        regions: Vec::new(),
        region: REGION_ALL,

        prefcond: vec![Bytes::default()],
        comp_max: MAXWLEN as u8,
        comp_minlen: 0,
        comp_sylmax: MAXWLEN as u8,
        comp_options: 0,
        comp_rules: CompoundRules::new(),
        comp_patterns: Vec::new(),
        syllable: Syllable::new(),
        nobreak: false,
        sal: None,
        map: None,
        rep: Vec::new(),
        rep_first: [-1; 256],
        repsal: Vec::new(),
        repsal_first: [-1; 256],
        common_words: CommonWords::new(),
    };

    // "ago" -> prefix "a" + "go", rare prefix -> ValidRare
    assert!(dict.check_word(b"ago"));
    assert!(dict.check_word(b"go"));
}

#[test]
fn test_sal_parsing() {
    let dict = load_dict();
    let sal = dict.sal.as_ref().expect("en dict should have SAL data");
    // English dictionary has 107 SAL rules + 1 sentinel.
    assert_eq!(sal.items.len(), 108);
    assert!(sal.followup);
    assert!(!sal.collapse);
    assert!(sal.rem_accents);
}

#[test]
fn test_sal_first_index() {
    let dict = load_dict();
    let sal = dict.sal.as_ref().unwrap();
    // There should be entries in the first-byte index.
    let count = sal.first.iter().filter(|&&x| x >= 0).count();
    assert!(count > 0, "should have some first-byte entries");
}

#[test]
fn test_soundfold_nonempty() {
    let dict = load_dict();
    let result = dict.soundfold(b"hello");
    assert!(
        !result.is_empty(),
        "soundfold should produce output for 'hello'"
    );
}

#[test]
fn test_soundfold_similar_words() {
    let dict = load_dict();
    // Words that sound alike should produce similar soundfolded forms.
    let sf_phone = dict.soundfold(b"phone");
    let sf_fone = dict.soundfold(b"fone");
    // Both should start with the same phonetic representation.
    assert!(
        !sf_phone.is_empty() && !sf_fone.is_empty(),
        "soundfold should produce output"
    );
    // They should score well against each other.
    let score = soundfold::soundalike_score(&sf_phone, &sf_fone);
    assert!(
        score < SCORE_MAXMAX,
        "phone and fone should be phonetically similar, got score {}",
        score
    );
}

#[test]
fn test_soundfold_different_words() {
    let dict = load_dict();
    let sf_cat = dict.soundfold(b"cat");
    let sf_umbrella = dict.soundfold(b"umbrella");
    // Very different words should score high.
    let score = soundfold::soundalike_score(&sf_cat, &sf_umbrella);
    assert!(
        score >= SCORE_MAXMAX,
        "cat and umbrella should be phonetically very different, got score {}",
        score
    );
}

#[test]
fn test_soundalike_score_identical() {
    assert_eq!(soundfold::soundalike_score(b"ABC", b"ABC"), 0);
    assert_eq!(soundfold::soundalike_score(b"", b""), 0);
}

#[test]
fn test_soundalike_score_one_char_diff() {
    // Swap: adjacent chars swapped.
    let score = soundfold::soundalike_score(b"AB", b"BA");
    assert_eq!(score, SCORE_SWAP);

    // Substitution: one char different, same length.
    let score = soundfold::soundalike_score(b"AX", b"AY");
    assert_eq!(score, SCORE_SUBST);
}

#[test]
fn test_soundalike_score_length_diff() {
    // One deletion.
    let score = soundfold::soundalike_score(b"AB", b"A");
    assert_eq!(score, SCORE_DEL);

    // Two deletions.
    let score = soundfold::soundalike_score(b"ABC", b"A");
    assert_eq!(score, SCORE_DEL * 2);
}

#[test]
fn test_soundalike_score_too_different() {
    let score = soundfold::soundalike_score(b"ABCDE", b"XY");
    assert_eq!(score, SCORE_MAXMAX);
}

#[test]
fn test_suggestions_with_sal() {
    let dict = load_dict();
    // The existing "sampl" -> "sample" should still work.
    let input = b"sampl";
    let typo = Typo { start: 0, end: 5 };
    let suggestions = dict.suggestions(&typo, input);
    assert!(
        suggestions.iter().any(|s| s == b"sample"),
        "should suggest 'sample' for 'sampl', got {:?}",
        suggestions
            .iter()
            .map(|s| String::from_utf8_lossy(s).to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_rep_parsing() {
    let dict = load_dict();
    assert!(
        !dict.rep.is_empty(),
        "en dict should have REP rules, got {}",
        dict.rep.len()
    );
    // Verify first-byte index has entries.
    let count = dict.rep_first.iter().filter(|&&x| x >= 0).count();
    assert!(
        count > 0,
        "should have some first-byte entries in rep_first"
    );
}

#[test]
fn test_rep_suggestions() {
    // Build a dictionary with a REP rule: "f" -> "ph"
    // Word tree contains "phone" so "fone" should be suggested as "phone" via REP.
    let mut arena = Arena::default();

    // Build foldtree containing "phone"
    let foldtree = WordTree {
        byts: vec![1, b'p', 1, b'h', 1, b'o', 1, b'n', 1, b'e', 1, 0],
        idxs: vec![0, 2, 0, 4, 0, 6, 0, 8, 0, 10, 0, 0],
    };

    let rep_from = arena.alloc(b"f");
    let rep_to = arena.alloc(b"ph");
    let rep = vec![RepItem {
        from: rep_from,
        to: rep_to,
    }];
    let mut rep_first = [-1i16; 256];
    rep_first[b'f' as usize] = 0;

    let dict = Dictionary {
        arena,
        foldtree,
        keeptree: WordTree::new(),
        prefixtree: WordTree::new(),
        charflags: CharFlags::new(),
        regions: Vec::new(),
        region: REGION_ALL,

        prefcond: Vec::new(),
        comp_max: MAXWLEN as u8,
        comp_minlen: 0,
        comp_sylmax: MAXWLEN as u8,
        comp_options: 0,
        comp_rules: CompoundRules::new(),
        comp_patterns: Vec::new(),
        syllable: Syllable::new(),
        nobreak: false,
        sal: None,
        map: None,
        rep,
        rep_first,
        repsal: Vec::new(),
        repsal_first: [-1; 256],
        common_words: CommonWords::new(),
    };

    let input = b"fone";
    let typo = Typo { start: 0, end: 4 };
    let suggestions = dict.suggestions(&typo, input);
    assert!(
        suggestions.iter().any(|s| s == b"phone"),
        "should suggest 'phone' for 'fone' via REP rule f->ph, got {:?}",
        suggestions
            .iter()
            .map(|s| String::from_utf8_lossy(s).to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_rep_score_is_low() {
    // REP suggestions should score lower (better) than substitutions.
    let mut arena = Arena::default();

    // Build foldtree containing "phase" and "phast"
    // "phase": p-h-a-s-e
    // "phast": p-h-a-s-t
    let foldtree = WordTree {
        byts: vec![
            1, b'p', 1, b'h', 1, b'a', 1, b's', 2, b'e', b't', 1, 0, 1, 0,
        ],
        idxs: vec![0, 2, 0, 4, 0, 6, 0, 8, 0, 11, 13, 0, 0, 0, 0],
    };

    let rep_from = arena.alloc(b"f");
    let rep_to = arena.alloc(b"ph");
    let rep = vec![RepItem {
        from: rep_from,
        to: rep_to,
    }];
    let mut rep_first = [-1i16; 256];
    rep_first[b'f' as usize] = 0;

    let dict = Dictionary {
        arena,
        foldtree,
        keeptree: WordTree::new(),
        prefixtree: WordTree::new(),
        charflags: CharFlags::new(),
        regions: Vec::new(),
        region: REGION_ALL,

        prefcond: Vec::new(),
        comp_max: MAXWLEN as u8,
        comp_minlen: 0,
        comp_sylmax: MAXWLEN as u8,
        comp_options: 0,
        comp_rules: CompoundRules::new(),
        comp_patterns: Vec::new(),
        syllable: Syllable::new(),
        nobreak: false,
        sal: None,
        map: None,
        rep,
        rep_first,
        repsal: Vec::new(),
        repsal_first: [-1; 256],
        common_words: CommonWords::new(),
    };

    let input = b"fase";
    let typo = Typo { start: 0, end: 4 };
    let suggestions = dict.suggestions(&typo, input);
    // "phase" should come before "phast" since "fase" -> REP "f"->"ph" -> "phase" is exact,
    // while "phast" requires both REP + substitution.
    assert!(
        !suggestions.is_empty() && suggestions[0] == b"phase",
        "REP match 'phase' should be ranked first for 'fase', got {:?}",
        suggestions
            .iter()
            .map(|s| String::from_utf8_lossy(s).to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_region_names() {
    let dict = load_dict();
    let regions = dict.region_names();
    // English dictionary should have regions (us, ca, etc.)
    assert!(
        !regions.is_empty(),
        "en dict should have region names defined"
    );
}

#[test]
fn test_region_filtering_synthetic() {
    // Build a dictionary where "colour" is only valid in region 1 ("gb"),
    // and "color" is valid in all regions.
    let arena = Arena::default();

    // "color" at bytes 0-5, no WF_REGION → valid everywhere
    // "colour" at bytes 6-12, WF_REGION set, region mask = 0x02 (bit 1 = "gb")
    //
    // Trie: root → c → o → l → o → (r → [end:no_region], u → r → [end:region=gb])
    let color_flags: u32 = 0; // no region, no special flags
    let colour_flags: u32 = (WF_REGION as u32) | (0x02u32 << 16); // WF_REGION + region mask in bits 16-23

    // Build a simple foldtree for "color" and "colour".
    //
    // [0]  = 1 (root: 1 sibling 'c')
    // [1]  = 'c', idx=2
    // [2]  = 1, 'o', idx=4
    // [4]  = 1, 'l', idx=6
    // [6]  = 1, 'o', idx=8
    // [8]  = 2 (2 siblings: 'r', 'u')
    // [9]  = 'r', idx=11 → [11]=1, [12]=0 (end, color_flags)
    // [10] = 'u', idx=13 → [13]=1, 'r', idx=15 → [15]=1, [16]=0 (end, colour_flags)
    let foldtree = WordTree {
        byts: vec![
            1, b'c', 1, b'o', 1, b'l', 1, b'o', 2, b'r', b'u', 1, 0, 1, b'r', 1, 0,
        ],
        idxs: vec![
            0,
            2,
            0,
            4,
            0,
            6,
            0,
            8,
            0,
            11,
            13,
            0,
            color_flags,
            0,
            15,
            0,
            colour_flags,
        ],
    };

    let mut dict = Dictionary {
        arena,
        foldtree,
        keeptree: WordTree::new(),
        prefixtree: WordTree::new(),
        charflags: CharFlags::new(),
        regions: vec![*b"us", *b"gb"],
        region: REGION_ALL,

        prefcond: Vec::new(),
        comp_max: MAXWLEN as u8,
        comp_minlen: 0,
        comp_sylmax: MAXWLEN as u8,
        comp_options: 0,
        comp_rules: CompoundRules::new(),
        comp_patterns: Vec::new(),
        syllable: Syllable::new(),
        nobreak: false,
        sal: None,
        map: None,
        rep: Vec::new(),
        rep_first: [-1; 256],
        repsal: Vec::new(),
        repsal_first: [-1; 256],
        common_words: CommonWords::new(),
    };

    // With REGION_ALL (default), both words are valid.
    assert!(dict.check_word(b"color"));
    assert!(dict.check_word(b"colour"));

    // Set region to "gb" (bit 1) — "colour" should be valid, "color" still valid (no region flag).
    dict.set_region(b"gb");
    assert!(dict.check_word(b"color"));
    assert!(dict.check_word(b"colour"));

    // Set region to "us" (bit 0) — "colour" should be rejected (region mismatch).
    dict.set_region(b"us");
    assert!(dict.check_word(b"color"));
    assert!(!dict.check_word(b"colour"));

    // Clear region — both valid again.
    dict.clear_region();
    assert!(dict.check_word(b"color"));
    assert!(dict.check_word(b"colour"));
}

#[test]
fn test_region_wrong_region_result() {
    let arena = Arena::default();

    // "grey" with WF_REGION, region mask = 0x02 (only region 1)
    let grey_flags: u32 = (WF_REGION as u32) | (0x02u32 << 16);
    let foldtree = WordTree {
        byts: vec![1, b'g', 1, b'r', 1, b'e', 1, b'y', 1, 0],
        idxs: vec![0, 2, 0, 4, 0, 6, 0, 8, 0, grey_flags],
    };

    let mut dict = Dictionary {
        arena,
        foldtree,
        keeptree: WordTree::new(),
        prefixtree: WordTree::new(),
        charflags: CharFlags::new(),
        regions: vec![*b"us", *b"gb"],
        region: REGION_ALL,

        prefcond: Vec::new(),
        comp_max: MAXWLEN as u8,
        comp_minlen: 0,
        comp_sylmax: MAXWLEN as u8,
        comp_options: 0,
        comp_rules: CompoundRules::new(),
        comp_patterns: Vec::new(),
        syllable: Syllable::new(),
        nobreak: false,
        sal: None,
        map: None,
        rep: Vec::new(),
        rep_first: [-1; 256],
        repsal: Vec::new(),
        repsal_first: [-1; 256],
        common_words: CommonWords::new(),
    };

    // REGION_ALL: valid
    assert!(dict.check_word(b"grey"));

    // Set to "gb" (bit 1): matches region mask 0x02 → valid
    dict.set_region(b"gb");
    assert!(dict.check_word(b"grey"));

    // Set to "us" (bit 0): doesn't match 0x02 → wrong region → check_word returns false
    dict.set_region(b"us");
    assert!(!dict.check_word(b"grey"));

    // But the word still exists — check_word_internal should return WrongRegion, not NotFound
    assert_eq!(dict.check_word_internal(b"grey"), WordResult::WrongRegion);
}

#[test]
fn test_region_set_unknown() {
    let mut dict = load_dict();
    // Setting an unknown region should fall back to REGION_ALL.
    dict.set_region(b"zz");
    // All normal words should still be valid.
    assert!(dict.check_word(b"hello"));
}

#[test]
fn test_region_suggestions_penalty() {
    // Build dict with "gray" (no region) and "grey" (region "gb" only).
    // When region is "us", "grey" should still be suggested for "gry" but
    // ranked lower than "gray" due to SCORE_REGION penalty.
    let arena = Arena::default();

    let gray_flags: u32 = 0;
    let grey_flags: u32 = (WF_REGION as u32) | (0x02u32 << 16);

    // Trie for "gray" and "grey":
    // root → g → r → (a → y → [end:gray], e → y → [end:grey])
    let foldtree = WordTree {
        byts: vec![
            1, b'g', 1, b'r', 2, b'a', b'e', 1, b'y', 1, 0, 1, b'y', 1, 0,
        ],
        idxs: vec![
            0, 2, 0, 4, 0, 7, 11, 0, 9, 0, gray_flags, 0, 13, 0, grey_flags,
        ],
    };

    let mut dict = Dictionary {
        arena,
        foldtree,
        keeptree: WordTree::new(),
        prefixtree: WordTree::new(),
        charflags: CharFlags::new(),
        regions: vec![*b"us", *b"gb"],
        region: REGION_ALL,

        prefcond: Vec::new(),
        comp_max: MAXWLEN as u8,
        comp_minlen: 0,
        comp_sylmax: MAXWLEN as u8,
        comp_options: 0,
        comp_rules: CompoundRules::new(),
        comp_patterns: Vec::new(),
        syllable: Syllable::new(),
        nobreak: false,
        sal: None,
        map: None,
        rep: Vec::new(),
        rep_first: [-1; 256],
        repsal: Vec::new(),
        repsal_first: [-1; 256],
        common_words: CommonWords::new(),
    };

    // With REGION_ALL, both should be suggested.
    let input = b"gry";
    let typo = Typo { start: 0, end: 3 };
    let suggestions = dict.suggestions(&typo, input);
    assert!(suggestions.iter().any(|s| s == b"gray"));
    assert!(suggestions.iter().any(|s| s == b"grey"));

    // With region "us", "grey" should still appear but "gray" should rank first.
    dict.set_region(b"us");
    let suggestions = dict.suggestions(&typo, input);
    assert!(suggestions.iter().any(|s| s == b"gray"));
    assert!(suggestions.iter().any(|s| s == b"grey"));
    // "gray" should be first (no region penalty) vs "grey" (SCORE_REGION penalty).
    let gray_pos = suggestions.iter().position(|s| s == b"gray").unwrap();
    let grey_pos = suggestions.iter().position(|s| s == b"grey").unwrap();
    assert!(
        gray_pos < grey_pos,
        "gray (no region penalty) should rank before grey (wrong region), \
             but gray_pos={}, grey_pos={}",
        gray_pos,
        grey_pos
    );
}

#[test]
fn test_char_roundtrip() {
    let cases: &[&str] = &["hello", "caf\u{e9}", "\u{2713}"];
    for &input in cases {
        let chars: Vec<char> = input.chars().collect();
        let output: String = chars.into_iter().collect();
        assert_eq!(input, output);
    }
}

#[test]
fn test_map_parsing() {
    let dict = load_dict();
    assert!(dict.has_map(), "English dictionary should have MAP data");
    let map = dict.map.as_ref().unwrap();
    assert!(
        map.map_array[b'a' as usize] != 0,
        "letter 'a' should have a MAP entry"
    );
}

#[test]
fn test_map_similar_chars() {
    let mut map_array = [0u32; 256];
    map_array[b'a' as usize] = b'a' as u32;
    map_array[b'e' as usize] = b'e' as u32;
    let map = MapInfo { map_array };
    let mut dict = Dictionary {
        arena: Arena::default(),
        foldtree: WordTree::new(),
        keeptree: WordTree::new(),
        prefixtree: WordTree::new(),
        charflags: CharFlags::new(),
        regions: Vec::new(),
        region: REGION_ALL,

        prefcond: Vec::new(),
        comp_max: MAXWLEN as u8,
        comp_minlen: 0,
        comp_sylmax: MAXWLEN as u8,
        comp_options: 0,
        comp_rules: CompoundRules::new(),
        comp_patterns: Vec::new(),
        syllable: Syllable::new(),
        nobreak: false,
        sal: None,
        map: Some(map),
        rep: Vec::new(),
        rep_first: [-1; 256],
        repsal: Vec::new(),
        repsal_first: [-1; 256],
        common_words: CommonWords::new(),
    };
    let m = &dict.map.as_ref().unwrap().map_array;
    // 'a' and 'e' have different group IDs, so not similar
    assert!(m[b'a' as usize] != m[b'e' as usize] || m[b'a' as usize] == 0);
    // 'x' and 'y' both 0, so not similar
    assert_eq!(m[b'x' as usize], 0);

    dict.map.as_mut().unwrap().map_array[b'e' as usize] = b'a' as u32;
    let m = &dict.map.as_ref().unwrap().map_array;
    // Now 'a' and 'e' share the same non-zero group ID
    assert!(m[b'a' as usize] != 0 && m[b'a' as usize] == m[b'e' as usize]);
}

#[test]
fn test_map_similar_substitution_score() {
    let arena = Arena::default();

    // Trie for "car" and "cat":
    // [0]=1 'c' -> [2]=1 'a' -> [4]=2 'r','t'
    //   'r' -> [7]=1 end(0) -> no flags
    //   't' -> [9]=1 end(0) -> no flags
    let foldtree = WordTree {
        byts: vec![1, b'c', 1, b'a', 2, b'r', b't', 1, 0, 1, 0],
        idxs: vec![0, 2, 0, 4, 0, 7, 9, 0, 0, 0, 0],
    };

    let mut map_array = [0u32; 256];
    map_array[b'r' as usize] = b'r' as u32;
    map_array[b't' as usize] = b'r' as u32;
    let map = MapInfo { map_array };

    let dict = Dictionary {
        arena,
        foldtree,
        keeptree: WordTree::new(),
        prefixtree: WordTree::new(),
        charflags: CharFlags::new(),
        regions: Vec::new(),
        region: REGION_ALL,

        prefcond: Vec::new(),
        comp_max: MAXWLEN as u8,
        comp_minlen: 0,
        comp_sylmax: MAXWLEN as u8,
        comp_options: 0,
        comp_rules: CompoundRules::new(),
        comp_patterns: Vec::new(),
        syllable: Syllable::new(),
        nobreak: false,
        sal: None,
        map: Some(map),
        rep: Vec::new(),
        rep_first: [-1; 256],
        repsal: Vec::new(),
        repsal_first: [-1; 256],
        common_words: CommonWords::new(),
    };

    let input = b"car";
    let typo = Typo { start: 0, end: 3 };
    let suggestions = dict.suggestions(&typo, input);
    assert!(
        suggestions.iter().any(|s| s == b"cat"),
        "should suggest 'cat' for 'car' when r/t are similar, got {:?}",
        suggestions
            .iter()
            .map(|s| String::from_utf8_lossy(s).to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_common_words_hash_table() {
    let mut arena = Arena::default();
    let mut words = CommonWords::with_capacity(3);

    let hello = arena.alloc(b"hello");
    let world = arena.alloc(b"world");
    let rust = arena.alloc(b"rust");

    words.insert(&arena, hello, 10);
    words.insert(&arena, world, 50);
    words.insert(&arena, rust, 200);

    assert_eq!(words.lookup(&arena, b"hello"), 10);
    assert_eq!(words.lookup(&arena, b"world"), 50);
    assert_eq!(words.lookup(&arena, b"rust"), 200);
    assert_eq!(words.lookup(&arena, b"missing"), 0);
}

#[test]
fn test_common_words_duplicate_insert() {
    let mut arena = Arena::default();
    let mut words = CommonWords::with_capacity(2);

    let w1 = arena.alloc(b"hello");
    words.insert(&arena, w1, 10);
    let w2 = arena.alloc(b"hello");
    words.insert(&arena, w2, 5);

    assert_eq!(words.lookup(&arena, b"hello"), 15);
}

#[test]
fn test_common_words_empty() {
    let arena = Arena::default();
    let words = CommonWords::new();
    assert!(words.is_empty());
    assert_eq!(words.lookup(&arena, b"anything"), 0);
}

#[test]
fn test_common_words_suggestion_boost() {
    let mut arena = Arena::default();

    // "bat" and "bet" are both valid words.
    // "bat" is a common word, "bet" is not.
    // For typo "bxt", both require one substitution (SCORE_SUBST=93).
    // "bat" should rank higher due to common word bonus.
    let foldtree = WordTree {
        byts: vec![1, b'b', 2, b'a', b'e', 1, b't', 1, 0, 1, b't', 1, 0],
        idxs: vec![0, 2, 0, 5, 9, 0, 7, 0, 0, 0, 11, 0, 0],
    };

    let bat = arena.alloc(b"bat");
    let mut common_words = CommonWords::with_capacity(1);
    common_words.insert(&arena, bat, 10);

    let dict = Dictionary {
        arena,
        foldtree,
        keeptree: WordTree::new(),
        prefixtree: WordTree::new(),
        charflags: CharFlags::new(),
        regions: Vec::new(),
        region: REGION_ALL,

        prefcond: Vec::new(),
        comp_max: MAXWLEN as u8,
        comp_minlen: 0,
        comp_sylmax: MAXWLEN as u8,
        comp_options: 0,
        comp_rules: CompoundRules::new(),
        comp_patterns: Vec::new(),
        syllable: Syllable::new(),
        nobreak: false,
        sal: None,
        map: None,
        rep: Vec::new(),
        rep_first: [-1; 256],
        repsal: Vec::new(),
        repsal_first: [-1; 256],
        common_words,
    };

    let input = b"bxt";
    let typo = Typo { start: 0, end: 3 };
    let suggestions = dict.suggestions(&typo, input);

    let bat_pos = suggestions.iter().position(|s| s == b"bat");
    let bet_pos = suggestions.iter().position(|s| s == b"bet");
    assert!(
        bat_pos.is_some() && bet_pos.is_some(),
        "both 'bat' and 'bet' should be suggested, got {:?}",
        suggestions
            .iter()
            .map(|s| String::from_utf8_lossy(s).to_string())
            .collect::<Vec<_>>()
    );
    assert!(
        bat_pos.unwrap() < bet_pos.unwrap(),
        "common word 'bat' should rank before 'bet'"
    );
}

#[test]
fn test_score_wordcount_adj_thresholds() {
    let mut arena = Arena::default();
    let mut common_words = CommonWords::with_capacity(3);

    let low = arena.alloc(b"low");
    let mid = arena.alloc(b"mid");
    let high = arena.alloc(b"high");
    common_words.insert(&arena, low, 5);
    common_words.insert(&arena, mid, 10);
    common_words.insert(&arena, high, 100);

    let dict = Dictionary {
        arena,
        foldtree: WordTree::new(),
        keeptree: WordTree::new(),
        prefixtree: WordTree::new(),
        charflags: CharFlags::new(),
        regions: Vec::new(),
        region: REGION_ALL,

        prefcond: Vec::new(),
        comp_max: MAXWLEN as u8,
        comp_minlen: 0,
        comp_sylmax: MAXWLEN as u8,
        comp_options: 0,
        comp_rules: CompoundRules::new(),
        comp_patterns: Vec::new(),
        syllable: Syllable::new(),
        nobreak: false,
        sal: None,
        map: None,
        rep: Vec::new(),
        rep_first: [-1; 256],
        repsal: Vec::new(),
        repsal_first: [-1; 256],
        common_words,
    };

    // count=5 < SCORE_THRES2(10): bonus = SCORE_COMMON1(30)
    assert_eq!(dict.score_wordcount_adj(100, b"low", false), 70);
    // count=10 >= SCORE_THRES2, < SCORE_THRES3(100): bonus = SCORE_COMMON2(40)
    assert_eq!(dict.score_wordcount_adj(100, b"mid", false), 60);
    // count=100 >= SCORE_THRES3: bonus = SCORE_COMMON3(50)
    assert_eq!(dict.score_wordcount_adj(100, b"high", false), 50);
    // unknown word: no adjustment
    assert_eq!(dict.score_wordcount_adj(100, b"unknown", false), 100);
    // bonus clamped to 0
    assert_eq!(dict.score_wordcount_adj(20, b"high", false), 0);
    // split: half bonus
    assert_eq!(dict.score_wordcount_adj(100, b"high", true), 75);
}

#[test]
fn test_suggest_swap() {
    let dict = load_dict();
    let input = b"hte";
    let typo = Typo { start: 0, end: 3 };
    let suggestions = dict.suggestions(&typo, input);
    assert!(
        suggestions.iter().any(|s| s == b"the"),
        "should suggest 'the' for 'hte' via swap, got {:?}",
        suggestions
            .iter()
            .map(|s| String::from_utf8_lossy(s).to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_suggest_multi_edit() {
    let dict = load_dict();
    let input = b"teh";
    let typo = Typo { start: 0, end: 3 };
    let suggestions = dict.suggestions(&typo, input);
    assert!(
        suggestions.iter().any(|s| s == b"the"),
        "should suggest 'the' for 'teh' via multi-edit, got {:?}",
        suggestions
            .iter()
            .map(|s| String::from_utf8_lossy(s).to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_suggest_word_split() {
    let dict = load_dict();
    let input = b"inthe";
    let typo = Typo { start: 0, end: 5 };
    let suggestions = dict.suggestions(&typo, input);
    assert!(
        suggestions.iter().any(|s| s == b"in the"),
        "should suggest 'in the' for 'inthe' via split, got {:?}",
        suggestions
            .iter()
            .map(|s| String::from_utf8_lossy(s).to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_suggest_deletion() {
    let dict = load_dict();
    let input = b"helllo";
    let typo = Typo { start: 0, end: 6 };
    let suggestions = dict.suggestions(&typo, input);
    assert!(
        suggestions.iter().any(|s| s == b"hello"),
        "should suggest 'hello' for 'helllo' via deletion, got {:?}",
        suggestions
            .iter()
            .map(|s| String::from_utf8_lossy(s).to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_suggest_insertion() {
    let dict = load_dict();
    let input = b"helo";
    let typo = Typo { start: 0, end: 4 };
    let suggestions = dict.suggestions(&typo, input);
    assert!(
        suggestions.iter().any(|s| s == b"hello"),
        "should suggest 'hello' for 'helo' via insertion, got {:?}",
        suggestions
            .iter()
            .map(|s| String::from_utf8_lossy(s).to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
#[ignore]
fn bench_suggest_perf() {
    let dict = load_dict();
    let typos: &[&[u8]] = &[
        b"sampl",
        b"hte",
        b"teh",
        b"helllo",
        b"helo",
        b"inthe",
        b"wrold",
        b"fone",
        b"accomodation",
        b"definately",
        b"occured",
        b"recieve",
        b"seperate",
        b"untill",
        b"wich",
        b"becuase",
        b"thier",
        b"foriegn",
    ];

    for &typo_word in typos {
        let t = Typo {
            start: 0,
            end: typo_word.len() as u32,
        };
        let start = std::time::Instant::now();
        let suggestions = dict.suggestions(&t, typo_word);
        let elapsed = start.elapsed();
        eprintln!(
            "{:<20} {:>2} suggestions  {:>8.3} ms",
            std::str::from_utf8(typo_word).unwrap(),
            suggestions.len(),
            elapsed.as_secs_f64() * 1000.0,
        );
    }
}

#[test]
#[ignore]
fn debug_inthe_deep_scoring() {
    let dict = load_dict();
    let typo_word = b"inthe";
    let t = Typo {
        start: 0,
        end: typo_word.len() as u32,
    };
    let results = dict.suggestions_debug(&t, typo_word);

    println!(
        "=== Rust deep ranking for 'inthe' ({} candidates) ===",
        results.len()
    );
    for (i, (word, pre_sal, sal, final_score)) in results.iter().enumerate() {
        let w = std::str::from_utf8(word).unwrap_or("???");
        let marker = match w {
            "in thee" | "on the" | "in they" => " <-- MISSING in Rust top25",
            "lithe" | "tithe" | "withe" => " <-- EXTRA in Rust top25",
            _ => "",
        };
        println!(
            "{:>3}: {:<20} pre_sal={:<4} sal={:<4} final={:<4}{}",
            i + 1,
            w,
            pre_sal,
            sal,
            final_score,
            marker,
        );
    }
}
