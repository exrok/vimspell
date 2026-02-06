struct NvimResult {
    suggestions: &'static [&'static str],
    typo: &'static str,
}

/// Extract using bench/run.sh
/// max_suggestions: 25
/// spelllang: "en"
/// spellsuggest: "best"
#[rustfmt::skip]
static NEOVIM_RESULTS: [NvimResult; 18] = [
    NvimResult {
        suggestions: &[
            "sample", "small", "simple", "sampler", "same", "sumps", "sump",
            "suppl", "ample", "sampan", "simply", "champs", "champ", "sampled",
            "samples", "same pl", "Sam pl", "SAM pl", "amps", "amyl", "camps",
            "campy", "damps", "lamps", "ramps",
        ],
        typo: "sampl",
    },
    NvimResult {
        suggestions: &[
            "he", "ht", "hate", "the", "ate", "rte", "hie", "hoe", "hue",
            "had", "hot", "HT", "get", "let", "set", "GTE", "has", "him",
            "his", "Rte", "Ste", "Ute", "Hae", "Hts", "at",
        ],
        typo: "hte",
    },
    NvimResult {
        suggestions: &[
            "the", "ten", "tea", "tee", "eh", "heh", "meh", "ted", "tel",
            "tech", "Te", "too", "tell", "she", "he", "they", "thee", "few",
            "her", "NEH", "new", "sea", "see", "Th", "Ted",
        ],
        typo: "teh",
    },
    NvimResult {
        suggestions: &[
            "hello", "hallo", "hullo", "hell", "halloo", "he'll", "cello",
            "jello", "hells", "hellos", "hell lo", "help", "hell's", "hellion",
            "halo", "Heller", "hello o", "tell", "well", "hell o", "hall",
            "heal", "heel", "hill", "hull",
        ],
        typo: "helllo",
    },
    NvimResult {
        suggestions: &[
            "hello", "help", "halo", "hell", "hole", "hero", "held", "helm",
            "helot", "he lo", "hallo", "heel", "hullo", "head", "here", "tell",
            "well", "her", "he", "hale", "hall", "heal", "hill", "hula",
            "hull",
        ],
        typo: "helo",
    },
    NvimResult {
        suggestions: &[
            "in the", "anther", "the", "nth", "In the", "IN the", "inane",
            "indie", "inter", "inure", "untie", "ante", "inch", "into", "int",
            "niche", "ninth", "anthem", "int he", "in thee", "inn the",
            "on the", "in they", "bathe", "lathe",
        ],
        typo: "inthe",
    },
    NvimResult {
        suggestions: &[
            "world", "wold", "would", "old", "word", "wield", "bold", "cold",
            "fold", "gold", "hold", "mold", "roly", "sold", "told", "weld",
            "wild", "road", "role", "roll", "rood", "wrote", "rod", "we old",
            "rowed",
        ],
        typo: "wrold",
    },
    NvimResult {
        suggestions: &[
            "phone", "one", "fine", "bone", "cone", "done", "fore", "gone",
            "hone", "lone", "none", "pone", "tone", "zone", "fond", "font",
            "foe", "phoney", "f one", "fen", "five", "food", "form", "line",
            "nine",
        ],
        typo: "fone",
    },
    NvimResult {
        suggestions: &[
            "accommodation", "accommodations", "accommodating",
            "accommodation's", "a accommodation", "accumulation",
            "accommodative", "accommodate on", "w accommodation",
            "y accommodation", "accommodate ion", "e accommodation",
            "i accommodation", "o accommodation", "u accommodation",
            "à accommodation", "b accommodation", "c accommodation",
            "d accommodation", "f accommodation", "g accommodation",
            "h accommodation", "j accommodation", "k accommodation",
            "l accommodation",
        ],
        typo: "accomodation",
    },
    NvimResult {
        suggestions: &[
            "definitely", "defiantly", "definably", "de finitely",
            "def innately", "delicately", "deviantly", "DE finitely",
            "def inanely", "def irately", "defiant Ely", "dee finitely",
            "define telly", "finitely", "defeatedly", "do finitely",
            "definitively", "defiant eely", "definable", "infinitely",
            "indefinitely", "definite lye", "deo finitely", "dew finitely",
            "die finitely",
        ],
        typo: "definately",
    },
    NvimResult {
        suggestions: &[
            "occurred", "accrued", "cured", "occur ed", "OCRed", "accused",
            "odoured", "scoured", "secured", "uncured", "accursed", "occupied",
            "occur Ed", "oi cured", "o cured", "occur red", "occurs",
            "occluded", "occulted", "obscured", "occur", "or cured",
            "co cured", "occur de", "occur ad",
        ],
        typo: "occured",
    },
    NvimResult {
        suggestions: &[
            "receive", "relieve", "reserve", "receiver", "deceive", "received",
            "receives", "recede", "recipe", "recite", "relive", "revive",
            "reeve", "reliever", "Recife", "re sieve", "res eve", "relieves",
            "believe", "relieved", "reprieve", "retrieve", "rice eve",
            "rehi eve", "rei eve",
        ],
        typo: "recieve",
    },
    NvimResult {
        suggestions: &[
            "separate", "desperate", "depurate", "operate", "septate",
            "serrate", "separated", "separates", "federate", "generate",
            "selenate", "sewerage", "venerate", "temperate", "suppurate",
            "see rate", "seep rate", "sere rate", "super ate", "s operate",
            "sprite", "separator", "sprat", "cooperate", "see pirate",
        ],
        typo: "seperate",
    },
    NvimResult {
        suggestions: &[
            "until", "untie", "still", "until l", "infill", "uncial", "unroll",
            "untick", "untied", "unties", "unwell", "untold", "utile",
            "anthill", "instill", "untiled", "unlit", "UN till", "till",
            "anti ll", "untie l", "unto ll", "una till", "uni till",
            "untie ll",
        ],
        typo: "untill",
    },
    NvimResult {
        suggestions: &[
            "which", "with", "wish", "witch", "rich", "wick", "winch", "was",
            "Wis", "week", "wees", "each", "Mich", "much", "Rich", "such",
            "will", "his", "is", "write", "wash", "watch", "beech", "leech",
            "vis",
        ],
        typo: "wich",
    },
    NvimResult {
        suggestions: &[
            "because", "be cause", "became", "bemuse", "be curse", "be case",
            "decease", "be cease", "be chase", "cause", "Be cause",
            "bee cause", "be Case", "beck's", "becks", "backache", "becomes",
            "Be curse", "beaus", "Be case", "bi cause", "bu cause", "be cues",
            "be causer", "bey cause",
        ],
        typo: "becuase",
    },
    NvimResult {
        suggestions: &[
            "their", "shier", "tier", "thief", "Thieu", "they", "the", "trier",
            "there", "other", "them", "then", "thing", "this", "three", "her",
            "thigh", "thee", "thew", "chair", "shear", "hair", "hear", "tear",
            "thinner",
        ],
        typo: "thier",
    },
    NvimResult {
        suggestions: &[
            "foreign", "fringe", "forego", "faring", "firing", "few reign",
            "freon", "arraign", "foo reign", "boring", "coring", "forage",
            "forager", "forcing", "fording", "forging", "forking", "forming",
            "goring", "poring", "feign", "frozen", "reign", "foreigner",
            "Noriega",
        ],
        typo: "foriegn",
    },
];

use super::*;
use std::collections::HashSet;

fn load_dict() -> Dictionary {
    let contents = std::fs::read("/code/vim-spell/en.utf-8.spl").expect("should read file");
    Dictionary::parse(&contents).expect("should parse dictionary")
}

/// Maximum number of Neovim suggestions that can be missing from Rust's
/// output for any single typo. Boundary-of-top-25 differences are expected
/// due to minor scoring divergences (missing soundfold tree walk, etc.).
const MAX_MISSING_PER_TYPO: usize = 1;

/// Maximum total missing suggestions across all 18 typos.
/// Current state: 6 typos with 1 missing each = 6 total.
const MAX_TOTAL_MISSING: usize = 6;

#[test]
pub fn compare() {
    let dict = load_dict();
    let rust_scored: Vec<_> = NEOVIM_RESULTS
        .iter()
        .map(|result| {
            dict.suggestions_scored(
                &Typo {
                    start: 0,
                    end: result.typo.len() as u32,
                },
                result.typo.as_bytes(),
            )
        })
        .collect();

    let rust_results: Vec<Vec<&str>> = rust_scored
        .iter()
        .map(|scored| {
            scored
                .iter()
                .map(|(w, _)| std::str::from_utf8(w).unwrap())
                .collect()
        })
        .collect();

    let rust_score_maps: Vec<std::collections::HashMap<&str, i32>> = rust_scored
        .iter()
        .map(|scored| {
            scored
                .iter()
                .map(|(w, s)| (std::str::from_utf8(w).unwrap(), *s))
                .collect()
        })
        .collect();

    let mut total_missing = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (i, (reference, rust_suggestions)) in
        NEOVIM_RESULTS.iter().zip(rust_results.iter()).enumerate()
    {
        let scores = &rust_score_maps[i];

        let nvim_set: HashSet<&str> = reference.suggestions.iter().copied().collect();
        let rust_set: HashSet<&str> = rust_suggestions.iter().copied().collect();

        let missing: Vec<&str> = nvim_set.difference(&rust_set).copied().collect();
        let extra: Vec<&str> = rust_set.difference(&nvim_set).copied().collect();
        let missing_count = missing.len();
        total_missing += missing_count;

        if missing_count == 0 && extra.is_empty() {
            println!("TYPO {:15} OK  (25/25 match)", reference.typo);
            continue;
        }

        println!(
            "TYPO {:15} {}/{} match  ({} missing, {} extra)",
            reference.typo,
            reference.suggestions.len() - missing_count,
            reference.suggestions.len(),
            missing_count,
            extra.len(),
        );
        for s in &missing {
            println!("  - {}", s);
        }
        for s in &extra {
            let score = scores.get(s).unwrap_or(&-1);
            println!("  + {} (score: {})", s, score);
        }

        if missing_count > MAX_MISSING_PER_TYPO {
            failures.push(format!(
                "'{}': {} missing (max {})",
                reference.typo, missing_count, MAX_MISSING_PER_TYPO
            ));
        }
    }

    println!("\nTotal missing: {}/{}", total_missing, MAX_TOTAL_MISSING);

    if total_missing > MAX_TOTAL_MISSING {
        failures.push(format!(
            "total missing {} exceeds max {}",
            total_missing, MAX_TOTAL_MISSING
        ));
    }

    if !failures.is_empty() {
        panic!("Neovim comparison regression:\n  {}", failures.join("\n  "));
    }
}
