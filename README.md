# vimspell - the rust crate.

The same great spell checking you love from Vim, but 3 times faster as a 100% safe Rust crate.

[![Crates.io](https://img.shields.io/crates/v/vimspell?style=flat-square)](https://crates.io/crates/vimspell)
[![Docs.rs](https://img.shields.io/docsrs/vimspell?style=flat-square)](https://docs.rs/vimspell/latest/vimspell/)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)

Fast spell checking using Vim's `.spl` dictionary files.

## Features

- Fast: More than 3 times faster than C implementation in neovim.
- Drop-in Vim compatibility: Uses the same `.spl` files as Vim (format version 50)
- Smart suggestions: Edit distance + phonetic similarity (SAL) + character similarity (MAP)
- Regional variants: Filter by region (US/UK English, etc.)
- User dictionaries: Add custom words or ban incorrect ones at runtime
- Pure Safe Rust: No C bindings, no unsafe code, single common dependency (`hashbrown`)

## Quick Start

```rust
use vimspell::Dictionary;

// Load a dictionary
let bytes = std::fs::read("en.utf-8.spl").unwrap();
let dict = Dictionary::parse(&bytes).unwrap();

// Check a word
if dict.check_word("hello") {
    println!("Correct!");
}

// Get suggestions for typos (top 25, max score 350)
for (word, score) in dict.suggestions("speling", 25, 350) {
    println!("{}: {}", word, score);
}

// Scan a document
let text = "This is a sampel text with mistakse.";
for range in dict.spell_check(text) {
    println!("Typo: {}", &text[range]);
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
dict.accept_word("rustdoc");
dict.accept_word("async");

// Ban common mistakes
dict.ban_word("alot");
dict.ban_word("irregardless");
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

### Tuning Suggestions

```rust
// Fewer results with a tighter score threshold for speed
let fast = dict.suggestions("recieve", 5, 200);

// More results with a looser threshold
let thorough = dict.suggestions("recieve", 25, 350);

for (word, score) in &thorough {
    println!("{}: {}", word, score);
}
```

## Limitations

- Only supports VIMspell format version 2 (the current standard since 2006)
- Maximum word length: 254 bytes
- Affix rules (prefixes/suffixes) are parsed but not used in suggestions yet

## Acknowledgments

Spell checking algorithm ported from [Neovim](https://github.com/neovim/neovim).
