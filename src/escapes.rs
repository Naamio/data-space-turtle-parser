// SPDX-FileCopyrightText: 2026 Omnifi Foundation
// SPDX-License-Identifier: MPL-2.0

//! Decoding Turtle's escape sequences.
//!
//! Turtle has three kinds. A numeric escape, `\uXXXX` or `\UXXXXXXXX`, may appear in strings
//! and in Internationalized Resource Identifiers (IRIs) written in angle brackets. A
//! character escape, such as `\n` or `\"`, may appear only in strings. A local-name escape,
//! such as `\-`, may appear only in the local part of a prefixed name. The contract carries
//! every term with its escapes decoded, so these functions run before a term leaves the
//! parser.

use std::str::Chars;

/// A description, in English, of the fault in an escape. The caller adds the span.
pub type EscapeError = String;

/// Decodes a numeric escape whose `u` or `U` has just been read from `characters`.
///
/// `\u` takes four hexadecimal digits and `\U` takes eight. The value must be a Unicode
/// scalar value: surrogates and values above `U+10FFFF` are refused.
pub fn numeric(marker: char, characters: &mut Chars<'_>) -> Result<char, EscapeError> {
	let length = if marker == 'u' { 4 } else { 8 };
	let digits: String = characters.by_ref().take(length).collect();
	if digits.chars().count() != length || !digits.chars().all(|digit| digit.is_ascii_hexdigit()) {
		return Err(format!(
			"`\\{marker}` must be followed by {length} hexadecimal digits"
		));
	}
	u32::from_str_radix(&digits, 16)
		.ok()
		.and_then(char::from_u32)
		.ok_or_else(|| format!("`\\{marker}{digits}` is not a Unicode scalar value"))
}

/// Decodes the escape that follows a backslash in a string: a character escape or a
/// numeric escape.
pub fn in_string(characters: &mut Chars<'_>) -> Result<char, EscapeError> {
	match characters.next() {
		Some('t') => Ok('\t'),
		Some('b') => Ok('\u{8}'),
		Some('n') => Ok('\n'),
		Some('r') => Ok('\r'),
		Some('f') => Ok('\u{c}'),
		Some('"') => Ok('"'),
		Some('\'') => Ok('\''),
		Some('\\') => Ok('\\'),
		Some(marker @ ('u' | 'U')) => numeric(marker, characters),
		Some(other) => Err(format!("`\\{other}` is not an escape sequence in a string")),
		None => Err("a string ends with a lone backslash".to_owned()),
	}
}

/// Whether `character` may follow a backslash in the local part of a prefixed name. Turtle
/// allows these so that a local name can contain characters that would otherwise end it.
pub const fn is_local_name_escape(character: char) -> bool {
	matches!(
		character,
		'_' | '~'
			| '.'
			| '-'
			| '!'
			| '$'
			| '&'
			| '\''
			| '('
			| ')'
			| '*'
			| '+'
			| ','
			| ';'
			| '='
			| '/'
			| '?'
			| '#'
			| '@'
			| '%'
	)
}
