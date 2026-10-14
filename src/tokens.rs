// SPDX-FileCopyrightText: 2026 Omnifi Foundation
// SPDX-License-Identifier: MPL-2.0

//! Splitting a Turtle document into tokens.
//!
//! The [`Lexer`] reads the document one token at a time, on request, so that the grammar can
//! write every statement before a lexical error and only then report the error. Each token
//! carries the byte range it came from. Escapes are decoded here, so the grammar never sees
//! a backslash.
//!
//! The terminal names follow the Turtle 1.2 grammar: `IRIREF`, `PNAME_LN`,
//! `BLANK_NODE_LABEL`, `LANG_DIR`, the four string forms, and the numeric literals.

use std::str::Chars;

use naamio_space_kit_rdf::contract::parser::SyntaxError;
use naamio_space_kit_rdf::contract::terms::{BaseDirection, Span};

use crate::escapes;

/// A token and the bytes of the document it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
	/// The kind of token, with the text it carries where it carries any.
	pub kind: TokenKind,
	/// The bytes it came from.
	pub span: Span,
}

/// The kinds of token in a Turtle document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
	/// An Internationalized Resource Identifier (IRI) in angle brackets, with escapes
	/// decoded.
	Iri(String),
	/// A prefixed name. A prefix declaration's `ex:` is one with an empty local part.
	PrefixedName {
		/// The prefix, without the colon.
		prefix: String,
		/// The local part, with escapes decoded.
		local: String,
	},
	/// A blank node label, without the `_:`.
	BlankNodeLabel(String),
	/// A quoted string, in any of Turtle's four forms, with escapes decoded.
	String(String),
	/// A word after `@`. After a string it is a language tag, possibly with a base direction;
	/// at the start of a statement, `prefix`, `base` or `version` begin a directive.
	At {
		/// The word, such as `en-GB` or `prefix`.
		word: String,
		/// The base direction written after `--`, if there is one.
		direction: Option<BaseDirection>,
	},
	/// An unquoted integer, such as `-42`, as written.
	Integer(String),
	/// An unquoted decimal, such as `3.14`, as written.
	Decimal(String),
	/// An unquoted double, such as `1.5e10`, as written.
	Double(String),
	/// A bare word: `a`, `true`, `false`, or a SPARQL-style directive keyword such as
	/// `PREFIX`.
	Keyword(String),
	/// `.`, ending a statement.
	Dot,
	/// `;`, starting another predicate for the same subject.
	Semicolon,
	/// `,`, starting another object for the same subject and predicate.
	Comma,
	/// `[`, starting a blank node property list.
	OpenBracket,
	/// `]`, ending a blank node property list.
	CloseBracket,
	/// `(`, starting a collection.
	OpenParenthesis,
	/// `)`, ending a collection.
	CloseParenthesis,
	/// `^^`, introducing a datatype.
	DatatypeMarker,
	/// `<<(`, starting a triple term.
	TripleTermStart,
	/// `)>>`, ending a triple term.
	TripleTermEnd,
	/// `<<`, starting a reified triple.
	ReifiedTripleStart,
	/// `>>`, ending a reified triple.
	ReifiedTripleEnd,
	/// `{|`, starting an annotation.
	AnnotationStart,
	/// `|}`, ending an annotation.
	AnnotationEnd,
	/// `~`, introducing a reifier.
	Tilde,
}

/// Reads tokens from a Turtle document, one at a time.
#[derive(Debug)]
pub struct Lexer<'text> {
	/// The document, decoded as UTF-8.
	text: &'text str,
	/// The byte offset of the next character to read.
	position: usize,
}

impl<'text> Lexer<'text> {
	/// Starts reading `text` from its first byte.
	pub const fn new(text: &'text str) -> Self {
		Self { text, position: 0 }
	}

	/// The span of the end of the document, for errors about something missing there.
	pub fn end(&self) -> Span {
		span(self.text.len(), self.text.len())
	}

	/// Reads the next token, or `None` at the end of the document.
	///
	/// # Errors
	///
	/// Returns a syntax error for text that is no Turtle token, such as an unclosed string
	/// or an IRI containing a space.
	pub fn next_token(&mut self) -> Result<Option<Token>, SyntaxError> {
		self.skip_whitespace_and_comments();
		let start = self.position;
		let Some(first) = self.peek(0) else {
			return Ok(None);
		};
		let kind = if let Some((kind, length)) = self.punctuation(first) {
			self.position += length;
			kind
		} else {
			self.longer_token(first, start)?
		};
		Ok(Some(Token {
			kind,
			span: span(start, self.position),
		}))
	}

	/// Reads a token that is more than fixed punctuation, starting with `first`.
	fn longer_token(&mut self, first: char, start: usize) -> Result<TokenKind, SyntaxError> {
		match first {
			'<' => self.iri(start),
			'"' | '\'' => self.string(first, start),
			'@' => self.at_word(start),
			'_' if self.peek(1) == Some(':') => self.blank_node_label(start),
			'+' | '-' | '0'..='9' | '.' => self.number(start),
			':' => self.name(start),
			character if is_name_start(character) => self.name(start),
			character => Err(error(
				format!("`{character}` cannot start a token"),
				start,
				start + character.len_utf8(),
			)),
		}
	}

	/// Fixed punctuation starting with `first`, and its length in bytes, or `None` if the
	/// text here is not punctuation.
	fn punctuation(&self, first: char) -> Option<(TokenKind, usize)> {
		let next = self.peek(1);
		Some(match (first, next) {
			('<', Some('<')) if self.peek(2) == Some('(') => (TokenKind::TripleTermStart, 3),
			('<', Some('<')) => (TokenKind::ReifiedTripleStart, 2),
			(')', Some('>')) if self.peek(2) == Some('>') => (TokenKind::TripleTermEnd, 3),
			('>', Some('>')) => (TokenKind::ReifiedTripleEnd, 2),
			('^', Some('^')) => (TokenKind::DatatypeMarker, 2),
			('{', Some('|')) => (TokenKind::AnnotationStart, 2),
			('|', Some('}')) => (TokenKind::AnnotationEnd, 2),
			('.', next) if !next.is_some_and(|character| character.is_ascii_digit()) => {
				(TokenKind::Dot, 1)
			}
			(';', _) => (TokenKind::Semicolon, 1),
			(',', _) => (TokenKind::Comma, 1),
			('[', _) => (TokenKind::OpenBracket, 1),
			(']', _) => (TokenKind::CloseBracket, 1),
			('(', _) => (TokenKind::OpenParenthesis, 1),
			(')', _) => (TokenKind::CloseParenthesis, 1),
			('~', _) => (TokenKind::Tilde, 1),
			_ => return None,
		})
	}

	/// Reads an IRI in angle brackets. Characters Turtle forbids in an IRI, such as spaces
	/// and quotation marks, are refused; numeric escapes are decoded.
	fn iri(&mut self, start: usize) -> Result<TokenKind, SyntaxError> {
		let mut characters = self.text[start + 1..].chars();
		let mut iri = String::new();
		loop {
			match characters.next() {
				Some('>') => break,
				Some('\\') => iri.push(self.iri_escape(start, &mut characters)?),
				Some(character) if is_forbidden_in_iri(character) => {
					let problem = format!("{character:?} is not allowed in an IRI");
					return Err(self.iri_error(start, &problem));
				}
				Some(character) => iri.push(character),
				None => return Err(self.iri_error(start, "the IRI is never closed with `>`")),
			}
		}
		self.position = self.offset_after(&characters);
		Ok(TokenKind::Iri(iri))
	}

	/// Decodes the escape after a backslash in an IRI that starts at `start`. Only numeric
	/// escapes are allowed in IRIs; `\n` and its kind belong to strings.
	fn iri_escape(&self, start: usize, characters: &mut Chars<'_>) -> Result<char, SyntaxError> {
		let marker = characters.next().unwrap_or(' ');
		if marker != 'u' && marker != 'U' {
			return Err(self.iri_error(start, "only `\\u` and `\\U` escapes are allowed in an IRI"));
		}
		escapes::numeric(marker, characters)
			.map_err(|message| error(message, start, self.offset_after(characters)))
	}

	/// The byte offset of the next character `characters` would read, which runs to the end
	/// of the document.
	fn offset_after(&self, characters: &Chars<'_>) -> usize {
		self.text.len() - characters.as_str().len()
	}

	/// The error for an IRI that starts at `start`, spanning to the end of its line.
	fn iri_error(&self, start: usize, problem: &str) -> SyntaxError {
		let end = self.text[start..]
			.find('\n')
			.map_or(self.text.len(), |offset| start + offset);
		error(
			format!("the IRI here cannot be read: {problem}"),
			start,
			end,
		)
	}

	/// Reads a string in any of Turtle's four forms, opened by `quote`.
	fn string(&mut self, quote: char, start: usize) -> Result<TokenKind, SyntaxError> {
		let long = self.peek(1) == Some(quote) && self.peek(2) == Some(quote);
		let opening = if long { 3 } else { 1 };
		let mut characters = self.text[start + opening..].chars();
		// A long string's closing quote is followed by two more, which belong to the string's
		// end rather than to its value.
		let remaining_closing_quotes = if long { 2 } else { 0 };
		let mut value = String::new();
		loop {
			match characters.next() {
				Some(character)
					if character == quote && closes(long, quote, characters.as_str()) =>
				{
					self.position = self.offset_after(&characters) + remaining_closing_quotes;
					return Ok(TokenKind::String(value));
				}
				Some('\\') => value.push(self.string_escape(start, &mut characters)?),
				Some('\n' | '\r') if !long => {
					return Err(error(
						"the string is never closed on its line".to_owned(),
						start,
						start + 1,
					));
				}
				Some(character) => value.push(character),
				None => {
					return Err(error(
						"the string is never closed".to_owned(),
						start,
						start + 1,
					));
				}
			}
		}
	}

	/// Decodes the escape after a backslash in a string that starts at `start`.
	fn string_escape(&self, start: usize, characters: &mut Chars<'_>) -> Result<char, SyntaxError> {
		escapes::in_string(characters)
			.map_err(|message| error(message, start, self.offset_after(characters)))
	}

	/// Reads the word after `@`: a language tag with an optional `--ltr` or `--rtl`, or a
	/// directive keyword.
	fn at_word(&mut self, start: usize) -> Result<TokenKind, SyntaxError> {
		self.position = start + 1;
		let word_start = self.position;
		self.take_while(|character| character.is_ascii_alphabetic());
		while self.peek(0) == Some('-')
			&& self
				.peek(1)
				.is_some_and(|character| character.is_ascii_alphanumeric())
		{
			self.position += 1;
			self.take_while(|character| character.is_ascii_alphanumeric());
		}
		let word = self.text[word_start..self.position].to_owned();
		if word.is_empty() {
			return Err(error(
				"`@` must be followed by a language tag".to_owned(),
				start,
				start + 1,
			));
		}
		let direction = self.direction(start)?;
		Ok(TokenKind::At { word, direction })
	}

	/// Reads `--ltr` or `--rtl` after a language tag, if it is there.
	fn direction(&mut self, start: usize) -> Result<Option<BaseDirection>, SyntaxError> {
		let rest = &self.text[self.position..];
		let direction = if rest.starts_with("--ltr") {
			BaseDirection::Ltr
		} else if rest.starts_with("--rtl") {
			BaseDirection::Rtl
		} else if rest.starts_with("--") {
			return Err(error(
				"a base direction must be `--ltr` or `--rtl`".to_owned(),
				start,
				self.position + 2,
			));
		} else {
			return Ok(None);
		};
		self.position += "--ltr".len();
		Ok(Some(direction))
	}

	/// Reads a blank node label after `_:`.
	fn blank_node_label(&mut self, start: usize) -> Result<TokenKind, SyntaxError> {
		self.position = start + 2;
		let label_start = self.position;
		if !self
			.peek(0)
			.is_some_and(|character| is_name_start(character) || character.is_ascii_digit())
		{
			return Err(error(
				"`_:` must be followed by a label".to_owned(),
				start,
				self.position,
			));
		}
		self.take_while(|character| is_name_character(character) || character == '.');
		self.give_back_trailing_dots(label_start);
		Ok(TokenKind::BlankNodeLabel(
			self.text[label_start..self.position].to_owned(),
		))
	}

	/// Reads an unquoted number: an integer, a decimal or a double, depending on whether it
	/// has a decimal point or an exponent.
	fn number(&mut self, start: usize) -> Result<TokenKind, SyntaxError> {
		self.position = start;
		if matches!(self.peek(0), Some('+' | '-')) {
			self.position += 1;
		}
		let integer_digits = self.take_while(|character| character.is_ascii_digit());
		let fraction = self.peek(0) == Some('.')
			&& self.peek(1).is_some_and(|character| {
				character.is_ascii_digit() || matches!(character, 'e' | 'E')
			});
		let fraction_digits = if fraction {
			self.position += 1;
			self.take_while(|character| character.is_ascii_digit())
		} else {
			0
		};
		if integer_digits + fraction_digits == 0 {
			return Err(error(
				"a number must have at least one digit".to_owned(),
				start,
				self.position.max(start + 1),
			));
		}
		let exponent = self.exponent(start)?;
		let lexical = self.text[start..self.position].to_owned();
		Ok(if exponent {
			TokenKind::Double(lexical)
		} else if fraction {
			TokenKind::Decimal(lexical)
		} else {
			TokenKind::Integer(lexical)
		})
	}

	/// Reads an exponent such as `e10` or `E-3`, if there is one.
	fn exponent(&mut self, start: usize) -> Result<bool, SyntaxError> {
		if !matches!(self.peek(0), Some('e' | 'E')) {
			return Ok(false);
		}
		self.position += 1;
		if matches!(self.peek(0), Some('+' | '-')) {
			self.position += 1;
		}
		if self.take_while(|character| character.is_ascii_digit()) == 0 {
			return Err(error(
				"an exponent must have at least one digit".to_owned(),
				start,
				self.position,
			));
		}
		Ok(true)
	}

	/// Reads a prefixed name or a bare keyword.
	fn name(&mut self, start: usize) -> Result<TokenKind, SyntaxError> {
		self.position = start;
		self.take_while(|character| is_name_character(character) || character == '.');
		if self.peek(0) == Some(':') {
			let prefix = self.text[start..self.position].to_owned();
			if prefix.ends_with('.') {
				return Err(error(
					"a prefix cannot end with `.`".to_owned(),
					start,
					self.position,
				));
			}
			self.position += 1;
			let local = self.local_name(start)?;
			return Ok(TokenKind::PrefixedName { prefix, local });
		}
		self.give_back_trailing_dots(start);
		Ok(TokenKind::Keyword(
			self.text[start..self.position].to_owned(),
		))
	}

	/// Reads the local part of a prefixed name, decoding its escapes. Percent-encoded
	/// characters stay encoded, as Turtle requires. A trailing `.` ends the statement rather
	/// than belonging to the name.
	fn local_name(&mut self, start: usize) -> Result<String, SyntaxError> {
		let mut local = String::new();
		// The length of the name to keep, and the position it ends at, as of the last part that
		// did not end with a dot.
		let mut kept_length = 0;
		let mut kept_position = self.position;
		while let Some(part) = self.local_name_part(start)? {
			local.push_str(&part);
			if !local.ends_with('.') {
				kept_length = local.len();
				kept_position = self.position;
			}
		}
		local.truncate(kept_length);
		self.position = kept_position;
		Ok(local)
	}

	/// Reads the next part of a local name: an escaped character, a percent-encoded
	/// character, or a plain one. Returns `None` once the next character cannot be part of
	/// the name.
	fn local_name_part(&mut self, start: usize) -> Result<Option<String>, SyntaxError> {
		let Some(character) = self.peek(0) else {
			return Ok(None);
		};
		let part = match character {
			'\\' => self.local_name_escape(start)?.to_string(),
			'%' => self.percent_encoding(start)?,
			character if is_name_character(character) || matches!(character, ':' | '.') => {
				self.position += character.len_utf8();
				character.to_string()
			}
			_ => return Ok(None),
		};
		Ok(Some(part))
	}

	/// Reads an escape such as `\-` in a local name, and returns the character it stands
	/// for.
	fn local_name_escape(&mut self, start: usize) -> Result<char, SyntaxError> {
		let escaped = self
			.peek(1)
			.filter(|&escaped| escapes::is_local_name_escape(escaped));
		let Some(escaped) = escaped else {
			let message = "this escape is not allowed in a local name".to_owned();
			return Err(error(message, start, self.position + 2));
		};
		self.position += 1 + escaped.len_utf8();
		Ok(escaped)
	}

	/// Reads a percent-encoded character such as `%20` in a local name. Turtle keeps these
	/// encoded, so the three characters are returned as written.
	fn percent_encoding(&mut self, start: usize) -> Result<String, SyntaxError> {
		let encoded = self
			.text
			.get(self.position..self.position + 3)
			.filter(|encoded| {
				encoded
					.chars()
					.skip(1)
					.all(|digit| digit.is_ascii_hexdigit())
			});
		let Some(encoded) = encoded.map(str::to_owned) else {
			let message = "`%` must be followed by two hexadecimal digits".to_owned();
			return Err(error(message, start, self.position + 1));
		};
		self.position += 3;
		Ok(encoded)
	}

	/// Skips spaces, tabs, line endings, and comments from `#` to the end of the line.
	fn skip_whitespace_and_comments(&mut self) {
		loop {
			self.take_while(|character| matches!(character, ' ' | '\t' | '\r' | '\n'));
			if self.peek(0) != Some('#') {
				return;
			}
			self.take_while(|character| character != '\n');
		}
	}

	/// Moves past every character that satisfies `accept`, and returns how many there were.
	fn take_while(&mut self, accept: impl Fn(char) -> bool) -> usize {
		let mut count = 0;
		while let Some(character) = self.peek(0).filter(|&character| accept(character)) {
			self.position += character.len_utf8();
			count += 1;
		}
		count
	}

	/// Moves back over dots at the end of what has been read since `start`, which belong to
	/// the statement's full stop rather than to a name or label.
	fn give_back_trailing_dots(&mut self, start: usize) {
		while self.position > start && self.text[..self.position].ends_with('.') {
			self.position -= 1;
		}
	}

	/// The character `ahead` characters after the current position, if the document has one.
	fn peek(&self, ahead: usize) -> Option<char> {
		self.text[self.position..].chars().nth(ahead)
	}
}

/// Whether the closing quote just read ends a string. A short string ends at its quote; a
/// long string ends only at three quotes in a row, so `rest` must start with two more.
fn closes(long: bool, quote: char, rest: &str) -> bool {
	!long || rest.chars().take(2).all(|character| character == quote) && rest.chars().count() >= 2
}

/// Whether `character` may begin a prefix, a keyword, or a local name.
fn is_name_start(character: char) -> bool {
	character.is_alphabetic() || character == '_'
}

/// Whether `character` may continue a prefix, a label, or a local name. This follows
/// Turtle's `PN_CHARS`, which adds digits, hyphens, the middle dot and combining marks to the
/// characters a name may begin with.
fn is_name_character(character: char) -> bool {
	is_name_start(character)
		|| character.is_ascii_digit()
		|| matches!(character, '-' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

/// Whether `character` is forbidden inside an IRI in angle brackets.
const fn is_forbidden_in_iri(character: char) -> bool {
	matches!(
		character,
		'\0'..=' ' | '<' | '"' | '{' | '}' | '|' | '^' | '`'
	)
}

/// A span from byte `start` to byte `end`.
fn span(start: usize, end: usize) -> Span {
	Span {
		start: u64::try_from(start).unwrap_or(u64::MAX),
		end: u64::try_from(end).unwrap_or(u64::MAX),
	}
}

/// A syntax error covering bytes `start` to `end`.
fn error(message: String, start: usize, end: usize) -> SyntaxError {
	SyntaxError {
		message,
		span: span(start, end),
	}
}
