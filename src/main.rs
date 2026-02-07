use vimspell::Dictionary;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let command = args.get(1).map(|s| s.as_str()).unwrap_or("check");

    match command {
        "compound-words" => dump_compound_words(),
        "check" => spell_check(),
        "profile" => profile_suggestions(),
        // "trace" => {
        //     let word = args.get(2).map(|s| s.as_str()).unwrap_or("accomodation");
        //     trace_suggestions(word);
        // }
        _ => {
            eprintln!("Usage: vim-spell [compound-words|check|profile|trace <word>]");
            std::process::exit(1);
        }
    }
}

fn load_dict() -> Dictionary {
    let contents = std::fs::read("/code/vimspell/dicts/en.utf-8.spl").expect("Failed to read file");
    Dictionary::parse(&contents).expect("Failed to parse dictionary")
}

fn dump_compound_words() {
    let dict = load_dict();
    let mut count = 0;
    dict.iter_compound_words(|word, flags| {
        let comp_flag = (flags >> 24) as u8;
        if let Ok(s) = std::str::from_utf8(word) {
            println!("{} (flag={})", s, char::from(comp_flag));
            count += 1;
        }
    });
    eprintln!("Total words with compound flags: {}", count);
}

fn profile_suggestions() {
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

    for _ in 0..20 {
        for &typo_word in typos {
            std::hint::black_box(dict.suggestions(typo_word));
        }
    }
}

// fn trace_suggestions(word: &str) {
//     let dict = load_dict();
//     let (suggestions, trace) = dict.suggestions_traced(word.as_bytes());

//     println!("=== Trace for '{}' ===\n", word);
//     println!("{}", trace);

//     println!("Top suggestions:");
//     for s in suggestions.iter().take(10) {
//         println!("  {}", String::from_utf8_lossy(s));
//     }
// }

fn spell_check() {
    let dict = load_dict();
    let input = b"This is a sampl text with an accomodation typo.";

    for range in dict.spell_check(input) {
        let word = &input[range.clone()];
        println!(
            "Typo found: '{}' at positions {}-{}",
            word.escape_ascii(),
            range.start,
            range.end
        );
        println!("Suggestions:");
        for suggestion in dict.suggestions(word) {
            println!(" - {}", suggestion.escape_ascii());
        }
    }
}
