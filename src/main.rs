use vim_spell::Dictionary;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let command = args.get(1).map(|s| s.as_str()).unwrap_or("check");

    match command {
        "compound-info" => show_compound_info(),
        "compound-words" => dump_compound_words(),
        "check" => {
            for _ in 0..1000 {
                spell_check()
            }
        }
        _ => {
            eprintln!("Usage: vim-spell [compound-info|compound-words|check]");
            std::process::exit(1);
        }
    }
}

fn load_dict() -> Dictionary {
    let contents = std::fs::read("/code/vim-spell/en.utf-8.spl").expect("Failed to read file");
    Dictionary::parse(&contents).expect("Failed to parse dictionary")
}

fn show_compound_info() {
    let dict = load_dict();
    let info = dict.compound_info();
    println!("Compound word support: {}", dict.has_compound_rules());
    println!("Max words in compound: {}", info.max_words);
    println!("Min part length: {}", info.min_part_len);
    println!("Max syllables: {}", info.max_syllables);
    println!("Number of rules: {}", info.rules_count);
    println!("Number of patterns: {}", info.patterns_count);
    println!(
        "Start flags: {:?}",
        info.start_flags
            .iter()
            .map(|&b| char::from(b))
            .collect::<Vec<_>>()
    );
    println!(
        "All flags: {:?}",
        info.all_flags
            .iter()
            .map(|&b| char::from(b))
            .collect::<Vec<_>>()
    );
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

fn spell_check() {
    let dict = load_dict();
    let input = b"This is a sampl text with a typo.";

    for typo in dict.spell_check(input) {
        let word = typo.word(input);
        println!(
            "Typo found: '{}' at positions {}-{}",
            word.escape_ascii(),
            typo.start,
            typo.end
        );
        println!("Suggestions:");
        for suggestion in dict.suggestions(&typo, input) {
            println!(" - {}", suggestion.escape_ascii());
        }
    }
}
