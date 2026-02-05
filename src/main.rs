use vim_spell::Dictionary;

fn main() {
    let contents = std::fs::read("/code/vim-spell/en.utf-8.spl").expect("Failed to read file");
    let dict = Dictionary::parse(&contents).expect("Failed to parse dictionary");

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
