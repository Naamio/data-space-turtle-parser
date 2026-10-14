// SPDX-FileCopyrightText: 2026 Omnifi Foundation
// SPDX-License-Identifier: MPL-2.0

//! Reading Turtle's grammar into statements, as written.
//!
//! The grammar is read by recursive descent over the tokens the [`Lexer`] produces, one
//! function for each production of the Turtle 1.2 grammar. Nothing is resolved: an
//! Internationalized Resource Identifier (IRI) is passed on absolute, relative or prefixed
//! as it was written, and a blank node by the document's label or a number the parser
//! assigns. What the grammar does decide is everything Turtle's shorthands stand for, such
//! as `a`, collections, property lists in square brackets, and the datatypes of unquoted
//! numbers.

use naamio_space_kit_rdf::Reading;
use naamio_space_kit_rdf::contract::parser::{ParseError, SyntaxError};
use naamio_space_kit_rdf::contract::terms::{
	Annotation, BlankNode, IriReference, Language, Literal, LocatedStatement, Node,
	PrefixDeclaration, PrefixedName, Quad, Span, Statement, Term, Triple,
};

use crate::tokens::{Lexer, Token, TokenKind};
use crate::vocabulary;

/// Reads a whole document, keeping the statements made before any error.
///
/// The contract has a parser deliver every statement it completes before it reports an
/// error, so the host can decide whether to keep them. This is the form the component uses.
#[must_use]
pub fn read(document: &[u8]) -> Reading {
	let text = match std::str::from_utf8(document) {
		Ok(text) => text.strip_prefix('\u{feff}').unwrap_or(text),
		Err(invalid) => {
			let offset = u64::try_from(invalid.valid_up_to()).unwrap_or(u64::MAX);
			return Reading {
				statements: Vec::new(),
				outcome: Err(ParseError::Syntax(SyntaxError {
					message: "the document is not valid UTF-8, which Turtle requires".to_owned(),
					span: Span {
						start: offset,
						end: offset.saturating_add(1),
					},
				})),
			};
		}
	};
	let bom = document.len() - text.len();
	let mut parser = Parser::new(text);
	let outcome = parser.document();
	let mut statements = parser.statements;
	if bom > 0 {
		shift_spans(&mut statements, bom);
	}
	Reading {
		statements,
		outcome: outcome.map_err(|error| shift_error(error, bom)),
	}
}

/// Reads a whole document and returns its statements, or the first error. The statements
/// made before an error are discarded; [`read`] keeps them.
///
/// # Errors
///
/// Returns the [`ParseError`] that stopped the reading.
pub fn parse(document: &[u8]) -> Result<Vec<LocatedStatement>, ParseError> {
	let reading = read(document);
	reading.outcome.map(|()| reading.statements)
}

/// An object the grammar has read: the term, the triples of any triple terms it uses, and
/// the bytes it came from.
#[derive(Debug)]
struct Object {
	/// The term.
	term: Term,
	/// The triples a triple term in `term` refers to, by position.
	triple_terms: Vec<Triple>,
	/// The bytes of the document the term came from.
	span: Span,
}

/// The state of reading one document.
#[derive(Debug)]
struct Parser<'text> {
	/// The lexer the grammar takes its tokens from.
	lexer: Lexer<'text>,
	/// The next token, once it has been looked at but not yet taken.
	lookahead: Option<Token>,
	/// The statements read so far.
	statements: Vec<LocatedStatement>,
	/// The number to give the next blank node the parser creates.
	next_blank_node: u64,
	/// The byte offset at which the last token taken ends. It closes the span of a construct
	/// whose last token has already been consumed by the production that read it.
	last_end: u64,
}

impl<'text> Parser<'text> {
	/// Starts reading `text`.
	const fn new(text: &'text str) -> Self {
		Self {
			lexer: Lexer::new(text),
			lookahead: None,
			statements: Vec::new(),
			next_blank_node: 0,
			last_end: 0,
		}
	}

	/// `turtleDoc ::= statement*`
	fn document(&mut self) -> Result<(), ParseError> {
		while self.peek()?.is_some() {
			self.statement()?;
		}
		Ok(())
	}

	/// `statement ::= directive | triples '.'`
	fn statement(&mut self) -> Result<(), ParseError> {
		let directive = match self.peek()? {
			Some(TokenKind::At { word, .. }) => Some((word.clone(), true)),
			Some(TokenKind::Keyword(word)) if is_directive_keyword(word) => {
				Some((word.to_ascii_lowercase(), false))
			}
			_ => None,
		};
		if let Some((word, needs_dot)) = directive {
			return self.directive(&word, needs_dot);
		}
		self.triples()?;
		self.expect(&TokenKind::Dot, "a statement must end with `.`")
			.map(|_| ())
	}

	/// `prefixID`, `base` and `version`, in their `@` forms, which end with `.`, and their
	/// SPARQL forms, which do not.
	fn directive(&mut self, word: &str, needs_dot: bool) -> Result<(), ParseError> {
		let keyword = self.take()?;
		let statement = match word {
			"prefix" => self.prefix_declaration()?,
			"base" => {
				Statement::Base(self.iri_reference_token("`base` must be followed by an IRI")?)
			}
			"version" => Statement::Version(self.version_string()?),
			other => {
				return Err(syntax(
					format!("`@{other}` is not a directive"),
					keyword.span,
				));
			}
		};
		if needs_dot {
			self.expect(&TokenKind::Dot, "a directive must end with `.`")?;
		}
		self.push(
			Span {
				start: keyword.span.start,
				end: self.last_end,
			},
			statement,
		);
		Ok(())
	}

	/// The prefix and namespace of a prefix declaration.
	fn prefix_declaration(&mut self) -> Result<Statement, ParseError> {
		let name = self.take()?;
		let TokenKind::PrefixedName { prefix, local } = name.kind else {
			return Err(syntax(
				"`prefix` must be followed by a prefix such as `ex:`".to_owned(),
				name.span,
			));
		};
		if !local.is_empty() {
			return Err(syntax(
				"a prefix declaration names a prefix, such as `ex:`, not a prefixed name"
					.to_owned(),
				name.span,
			));
		}
		let namespace =
			self.iri_reference_token("a prefix must be followed by its namespace IRI")?;
		Ok(Statement::Prefix(PrefixDeclaration { prefix, namespace }))
	}

	/// The quoted version after `VERSION`.
	fn version_string(&mut self) -> Result<String, ParseError> {
		let token = self.take()?;
		match token.kind {
			TokenKind::String(version) => Ok(version),
			_ => Err(syntax(
				"`version` must be followed by a quoted version".to_owned(),
				token.span,
			)),
		}
	}

	/// `triples ::= subject predicateObjectList | blankNodePropertyList
	/// predicateObjectList? | reifiedTriple predicateObjectList?`
	fn triples(&mut self) -> Result<(), ParseError> {
		if self.peek_is(&TokenKind::OpenBracket)? {
			let (subject, _) = self.blank_node_property_list()?;
			if self.peek_is(&TokenKind::Dot)? {
				return Ok(());
			}
			return self.predicate_object_list(&subject);
		}
		if self.peek_is(&TokenKind::ReifiedTripleStart)? {
			return Err(unsupported("reified triples, written `<< … >>`"));
		}
		let subject = self.subject()?;
		self.predicate_object_list(&subject)
	}

	/// `subject ::= iri | BlankNode | collection`
	fn subject(&mut self) -> Result<Node, ParseError> {
		if self.peek_is(&TokenKind::OpenParenthesis)? {
			return Ok(self.collection()?.0);
		}
		let token = self.take()?;
		let span = token.span;
		match into_iri_reference(token.kind) {
			Ok(reference) => Ok(Node::Iri(reference)),
			Err(TokenKind::BlankNodeLabel(label)) => Ok(Node::BlankNode(BlankNode::Written(label))),
			Err(_) => Err(syntax(
				"a subject must be an IRI or a blank node".to_owned(),
				span,
			)),
		}
	}

	/// `predicateObjectList ::= verb objectList (';' (verb objectList)?)*`
	fn predicate_object_list(&mut self, subject: &Node) -> Result<(), ParseError> {
		loop {
			let predicate = self.verb()?;
			self.object_list(subject, &predicate)?;
			if !self.peek_is(&TokenKind::Semicolon)? {
				return Ok(());
			}
			while self.peek_is(&TokenKind::Semicolon)? {
				self.take()?;
			}
			if !self.peek_starts_verb()? {
				return Ok(());
			}
		}
	}

	/// `verb ::= predicate | 'a'`
	fn verb(&mut self) -> Result<IriReference, ParseError> {
		let token = self.take()?;
		let span = token.span;
		match into_iri_reference(token.kind) {
			Ok(reference) => Ok(reference),
			Err(TokenKind::Keyword(word)) if word == "a" => Ok(absolute(vocabulary::RDF_TYPE)),
			Err(_) => Err(syntax("a predicate must be an IRI or `a`".to_owned(), span)),
		}
	}

	/// `objectList ::= object annotation? (',' object annotation?)*`, writing one quad for
	/// each object.
	fn object_list(&mut self, subject: &Node, predicate: &IriReference) -> Result<(), ParseError> {
		loop {
			let object = self.object()?;
			if self.peek_is(&TokenKind::AnnotationStart)? || self.peek_is(&TokenKind::Tilde)? {
				return Err(unsupported(
					"annotations and reifiers, written `{| … |}` and `~`",
				));
			}
			self.push_quad(subject.clone(), predicate.clone(), object);
			if !self.peek_is(&TokenKind::Comma)? {
				return Ok(());
			}
			self.take()?;
		}
	}

	/// `object ::= iri | BlankNode | collection | blankNodePropertyList | literal |
	/// tripleTerm | reifiedTriple`
	fn object(&mut self) -> Result<Object, ParseError> {
		match self.peek()? {
			Some(TokenKind::OpenBracket) => {
				return self.blank_node_property_list().map(node_object);
			}
			Some(TokenKind::OpenParenthesis) => return self.collection().map(node_object),
			Some(TokenKind::TripleTermStart) => return self.triple_term_object(),
			Some(TokenKind::ReifiedTripleStart) => {
				return Err(unsupported("reified triples, written `<< … >>`"));
			}
			_ => {}
		}
		let (term, span) =
			self.simple_object("an object must be an IRI, a blank node or a literal")?;
		Ok(Object {
			term,
			triple_terms: Vec::new(),
			span,
		})
	}

	/// An object that is a single token, or a literal with its annotation: an IRI, a blank
	/// node label, a string, a number or a boolean.
	fn simple_object(&mut self, expected: &str) -> Result<(Term, Span), ParseError> {
		let token = self.take()?;
		let span = token.span;
		match into_iri_reference(token.kind) {
			Ok(reference) => Ok((Term::Iri(reference), span)),
			Err(TokenKind::BlankNodeLabel(label)) => {
				Ok((Term::BlankNode(BlankNode::Written(label)), span))
			}
			Err(TokenKind::String(lexical_form)) => self.string_literal(lexical_form, span),
			Err(TokenKind::Integer(lexical)) => Ok((typed(lexical, vocabulary::XSD_INTEGER), span)),
			Err(TokenKind::Decimal(lexical)) => Ok((typed(lexical, vocabulary::XSD_DECIMAL), span)),
			Err(TokenKind::Double(lexical)) => Ok((typed(lexical, vocabulary::XSD_DOUBLE), span)),
			Err(TokenKind::Keyword(word)) if word == "true" || word == "false" => {
				Ok((typed(word, vocabulary::XSD_BOOLEAN), span))
			}
			Err(_) => Err(syntax(expected.to_owned(), span)),
		}
	}

	/// `RDFLiteral ::= String (LANG_DIR | '^^' iri)?`, given the string already read.
	fn string_literal(
		&mut self,
		lexical_form: String,
		span: Span,
	) -> Result<(Term, Span), ParseError> {
		let annotation = match self.peek()? {
			Some(TokenKind::At { .. }) => {
				let token = self.take()?;
				let TokenKind::At { word, direction } = token.kind else {
					return Err(syntax("a language tag was expected".to_owned(), token.span));
				};
				Some((
					Annotation::Language(Language {
						tag: word,
						direction,
					}),
					token.span,
				))
			}
			Some(TokenKind::DatatypeMarker) => {
				self.take()?;
				let datatype = self.take()?;
				let datatype_span = datatype.span;
				let reference = into_iri_reference(datatype.kind).map_err(|_| {
					syntax(
						"`^^` must be followed by a datatype IRI".to_owned(),
						datatype_span,
					)
				})?;
				Some((Annotation::Datatype(reference), datatype_span))
			}
			_ => None,
		};
		let end = annotation
			.as_ref()
			.map_or(span, |(_, annotation_span)| *annotation_span);
		let literal = Term::Literal(Literal {
			lexical_form,
			annotation: annotation.map(|(annotation, _)| annotation),
		});
		Ok((
			literal,
			Span {
				start: span.start,
				end: end.end,
			},
		))
	}

	/// `blankNodePropertyList ::= '[' predicateObjectList ']'`, and the empty `[]`, which
	/// both name a new blank node. Returns the node and the span of the brackets. The
	/// property list's quads are written before the quad that uses the node, because they
	/// are complete first.
	fn blank_node_property_list(&mut self) -> Result<(Node, Span), ParseError> {
		let opening = self.take()?;
		let subject = Node::BlankNode(self.new_blank_node());
		if !self.peek_is(&TokenKind::CloseBracket)? {
			self.predicate_object_list(&subject)?;
		}
		let closing = self.expect(
			&TokenKind::CloseBracket,
			"a property list must end with `]`",
		)?;
		Ok((
			subject,
			Span {
				start: opening.span.start,
				end: closing.span.end,
			},
		))
	}

	/// `collection ::= '(' object* ')'`, written as the list RDF defines: a new blank node
	/// for each item, linked by `rdf:first` and `rdf:rest`, ending in `rdf:nil`. Returns the
	/// list's first node, or `rdf:nil` for an empty collection, and the span of the
	/// parentheses.
	fn collection(&mut self) -> Result<(Node, Span), ParseError> {
		let opening = self.take()?;
		let mut items = Vec::new();
		while !self.peek_is(&TokenKind::CloseParenthesis)? {
			if self.peek()?.is_none() {
				return Err(syntax(
					"the collection is never closed with `)`".to_owned(),
					opening.span,
				));
			}
			items.push(self.object()?);
		}
		let closing = self.take()?;
		let span = Span {
			start: opening.span.start,
			end: closing.span.end,
		};
		let nodes: Vec<BlankNode> = items.iter().map(|_| self.new_blank_node()).collect();
		let Some(head) = nodes.first().cloned() else {
			return Ok((Node::Iri(absolute(vocabulary::RDF_NIL)), span));
		};
		for (index, item) in items.into_iter().enumerate() {
			let list_node = Node::BlankNode(nodes[index].clone());
			let rest = nodes.get(index + 1).map_or_else(
				|| Term::Iri(absolute(vocabulary::RDF_NIL)),
				|next| Term::BlankNode(next.clone()),
			);
			let item_span = item.span;
			self.push_quad(list_node.clone(), absolute(vocabulary::RDF_FIRST), item);
			self.push_quad(
				list_node,
				absolute(vocabulary::RDF_REST),
				Object {
					term: rest,
					triple_terms: Vec::new(),
					span: item_span,
				},
			);
		}
		Ok((Node::BlankNode(head), span))
	}

	/// A triple term in the object position. Its triple, and any nested inside it, go into
	/// a list carried by the quad, and the term refers to its triple by position.
	fn triple_term_object(&mut self) -> Result<Object, ParseError> {
		let start = self.peek_span()?.start;
		let mut triple_terms = Vec::new();
		let position = self.triple_term(&mut triple_terms)?;
		Ok(Object {
			term: Term::TripleTerm(position),
			triple_terms,
			span: Span {
				start,
				end: self.last_end,
			},
		})
	}

	/// `tripleTerm ::= '<<(' ttSubject verb ttObject ')>>'`. Appends the triple to `list`
	/// after any triple terms nested in its object, and returns its position.
	fn triple_term(&mut self, list: &mut Vec<Triple>) -> Result<u32, ParseError> {
		self.take()?;
		let subject_token = self.take()?;
		let subject_span = subject_token.span;
		let subject = match into_iri_reference(subject_token.kind) {
			Ok(reference) => Node::Iri(reference),
			Err(TokenKind::BlankNodeLabel(label)) => Node::BlankNode(BlankNode::Written(label)),
			Err(_) => {
				return Err(syntax(
					"a triple term's subject must be an IRI or a blank node".to_owned(),
					subject_span,
				));
			}
		};
		let predicate = self.verb()?;
		let object = if self.peek_is(&TokenKind::TripleTermStart)? {
			Term::TripleTerm(self.triple_term(list)?)
		} else {
			self.simple_object(
				"a triple term's object must be an IRI, a blank node, a literal or a triple term",
			)?
			.0
		};
		self.expect(
			&TokenKind::TripleTermEnd,
			"a triple term must end with `)>>`",
		)?;
		list.push(Triple {
			subject,
			predicate,
			object,
		});
		u32::try_from(list.len() - 1)
			.map_err(|_| syntax("a quad has too many triple terms".to_owned(), subject_span))
	}

	/// An IRI reference from the next token: an IRI in angle brackets or a prefixed name.
	fn iri_reference_token(&mut self, expected: &str) -> Result<IriReference, ParseError> {
		let token = self.take()?;
		let span = token.span;
		into_iri_reference(token.kind).map_err(|_| syntax(expected.to_owned(), span))
	}

	/// The next blank node the parser creates for syntax that names no label.
	const fn new_blank_node(&mut self) -> BlankNode {
		let number = self.next_blank_node;
		self.next_blank_node += 1;
		BlankNode::Generated(number)
	}

	/// Writes a quad, with the span of the object that completed it.
	fn push_quad(&mut self, subject: Node, predicate: IriReference, object: Object) {
		let statement = Statement::Quad(Quad {
			subject,
			predicate,
			object: object.term,
			graph_name: None,
			triple_terms: object.triple_terms,
		});
		self.push(object.span, statement);
	}

	/// Writes a statement read from `span`.
	fn push(&mut self, span: Span, statement: Statement) {
		self.statements.push(LocatedStatement { span, statement });
	}

	/// Takes the next token, which must be `kind`.
	fn expect(&mut self, kind: &TokenKind, problem: &str) -> Result<Token, ParseError> {
		match self.peek()? {
			Some(next) if next == kind => self.take(),
			_ => {
				let span = self.peek_span()?;
				Err(syntax(problem.to_owned(), span))
			}
		}
	}

	/// Takes the next token, or reports that the document ended too soon.
	fn take(&mut self) -> Result<Token, ParseError> {
		self.peek()?;
		let token = self.lookahead.take().ok_or_else(|| {
			syntax(
				"the document ends in the middle of a statement".to_owned(),
				self.lexer.end(),
			)
		})?;
		self.last_end = token.span.end;
		Ok(token)
	}

	/// The kind of the next token, without taking it, or `None` at the end of the document.
	fn peek(&mut self) -> Result<Option<&TokenKind>, ParseError> {
		if self.lookahead.is_none() {
			self.lookahead = self.lexer.next_token().map_err(ParseError::Syntax)?;
		}
		Ok(self.lookahead.as_ref().map(|token| &token.kind))
	}

	/// Whether the next token is `kind`.
	fn peek_is(&mut self, kind: &TokenKind) -> Result<bool, ParseError> {
		Ok(self.peek()? == Some(kind))
	}

	/// Whether the next token can begin a verb, which decides whether a `;` is followed by
	/// another predicate or by the end of the list.
	fn peek_starts_verb(&mut self) -> Result<bool, ParseError> {
		Ok(matches!(
			self.peek()?,
			Some(TokenKind::Iri(_) | TokenKind::PrefixedName { .. })
		) || matches!(self.peek()?, Some(TokenKind::Keyword(word)) if word == "a"))
	}

	/// The span of the next token, or of the end of the document.
	fn peek_span(&mut self) -> Result<Span, ParseError> {
		self.peek()?;
		Ok(self
			.lookahead
			.as_ref()
			.map_or_else(|| self.lexer.end(), |token| token.span))
	}
}

/// The object a collection or property list stands for: the node that begins it, as a
/// term, with the span of its brackets.
fn node_object((node, span): (Node, Span)) -> Object {
	let term = match node {
		Node::BlankNode(blank_node) => Term::BlankNode(blank_node),
		Node::Iri(reference) => Term::Iri(reference),
	};
	Object {
		term,
		triple_terms: Vec::new(),
		span,
	}
}

/// Whether a bare word begins a SPARQL-style directive, which Turtle matches without regard
/// to case.
fn is_directive_keyword(word: &str) -> bool {
	["prefix", "base", "version"]
		.iter()
		.any(|keyword| word.eq_ignore_ascii_case(keyword))
}

/// The IRI reference a token stands for, or the token back if it is not an IRI. An IRI in
/// angle brackets is absolute if it starts with a scheme and relative otherwise.
fn into_iri_reference(kind: TokenKind) -> Result<IriReference, TokenKind> {
	match kind {
		TokenKind::Iri(iri) if has_scheme(&iri) => Ok(IriReference::Absolute(iri)),
		TokenKind::Iri(iri) => Ok(IriReference::Relative(iri)),
		TokenKind::PrefixedName { prefix, local } => {
			Ok(IriReference::Prefixed(PrefixedName { prefix, local }))
		}
		other => Err(other),
	}
}

/// Whether an IRI starts with a scheme, `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ) ":"`,
/// which is what makes it absolute in RFC 3986.
fn has_scheme(iri: &str) -> bool {
	let Some((scheme, _)) = iri.split_once(':') else {
		return false;
	};
	let mut characters = scheme.chars();
	characters
		.next()
		.is_some_and(|first| first.is_ascii_alphabetic())
		&& characters.all(|character| {
			character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
		})
}

/// An absolute IRI reference.
fn absolute(iri: &str) -> IriReference {
	IriReference::Absolute(iri.to_owned())
}

/// A literal whose datatype is the absolute IRI `datatype`, for the grammar's shorthand
/// numbers and booleans.
fn typed(lexical_form: String, datatype: &str) -> Term {
	Term::Literal(Literal {
		lexical_form,
		annotation: Some(Annotation::Datatype(absolute(datatype))),
	})
}

/// A syntax error covering `span`.
const fn syntax(message: String, span: Span) -> ParseError {
	ParseError::Syntax(SyntaxError { message, span })
}

/// The error for a feature of Turtle 1.2 this parser does not read yet.
fn unsupported(feature: &str) -> ParseError {
	ParseError::Unsupported(feature.to_owned())
}

/// Moves every span in `statements` on by `offset` bytes, to account for a byte order mark
/// removed before reading.
fn shift_spans(statements: &mut [LocatedStatement], offset: usize) {
	let offset = u64::try_from(offset).unwrap_or(0);
	for located in statements {
		located.span = Span {
			start: located.span.start + offset,
			end: located.span.end + offset,
		};
	}
}

/// Moves the span of a syntax error on by `offset` bytes, as [`shift_spans`] does.
fn shift_error(error: ParseError, offset: usize) -> ParseError {
	let offset = u64::try_from(offset).unwrap_or(0);
	match error {
		ParseError::Syntax(SyntaxError { message, span }) => syntax(
			message,
			Span {
				start: span.start + offset,
				end: span.end + offset,
			},
		),
		other => other,
	}
}
