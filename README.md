# vimspell - the rust crate.

The same great spell checking you love from Vim, but twice as fast and 100% safe, in an easily embeddable Rust crate.

Fast spell checking using Vim's `.spl` dictionary files.

## Features

- Fast: More than twice as fast as the C implementation in neovim.
- Drop-in Vim compatibility: Uses the same `.spl` files as Vim (format version 50)
- Smart suggestions: Edit distance + phonetic similarity (SAL) + character similarity (MAP)
- Regional variants: Filter by region (US/UK English, etc.)
- User dictionaries: Add custom words or ban incorrect ones at runtime
- Pure Rust: No C bindings, no unsafe code, single dependency (`hashbrown`)

## Quick Start

```rust
use vimspell::Dictionary;

// Load a dictionary
let bytes = std::fs::read("en.utf-8.spl").unwrap();
let dict = Dictionary::parse(&bytes).unwrap();

// Check a word
if dict.check_word(b"hello") {
    println!("Correct!");
}

// Get suggestions for typos
let suggestions = dict.suggestions(b"speling");
println!("Did you mean: {:?}", suggestions);
// Output: ["spelling", "spieling", "peeling", ...]

// Scan a document
let text = b"This is a sampel text with mistakse.";
for range in dict.spell_check(text) {
    println!("Typo at {}: {}", range.start, String::from_utf8_lossy(&text[range]));
}
```

## Getting Dictionary Files

Download pre-built `.spl` files from nlugg mirror for vim.

```bash
# English
curl -O ftp://ftp.nluug.nl/pub/vim/runtime/spell/en.utf-8.spl
# German
curl -O ftp://ftp.nlugg.nl/pub/vim/runtime/spell/de.utf-8.spl
# Spanish
curl -O ftp://ftp.nlugg.nl/pub/vim/runtime/spell/es.utf-8.spl
```

Or create your own with Neovim's `:mkspell` command.

## Usage Examples

### Custom User Dictionary

```rust
let mut dict = Dictionary::parse(&bytes).unwrap();

// Add technical terms
dict.add_good_word(b"rustdoc");
dict.add_good_word(b"async");

// Ban common mistakes
dict.ban_word(b"alot");
dict.ban_word(b"irregardless");
```

### Regional Preferences

```rust
// Set to US English
dict.set_region(b"us");

// Or UK English
dict.set_region(b"gb");

// Accept all regions
dict.clear_region();
```

### Get Scored Suggestions

```rust
let scored = dict.suggestions_scored(b"recieve");
for (word, score) in scored.iter().take(5) {
    println!("{}: {}", word.escape_ascii(), score);
}
// Output:
// receive: 65
// deceive: 130
// ...
```

## Limitations

- Only supports VIMspell format version 50 (the current standard)
- Maximum word length: 254 bytes
- Affix rules (prefixes/suffixes) are parsed but not used in suggestions yet

## Acknowledgments

Spell checking algorithm ported from [Neovim](https://github.com/neovim/neovim).
