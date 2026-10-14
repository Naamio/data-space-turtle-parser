// SPDX-FileCopyrightText: 2026 Omnifi Foundation
// SPDX-License-Identifier: MPL-2.0

//! A `linked-data:rdf` parser component for Turtle.
//!
//! The component knows the Turtle syntax and nothing more. It reads a document and streams
//! back the statements the document makes, as written: Internationalized Resource
//! Identifiers (IRIs) as they appear, absolute, relative or prefixed, blank nodes by their
//! labels or by numbers it assigns, and literals as text with a datatype or a language. It
//! resolves nothing and stores nothing; the host that runs it owns what the statements
//! mean.
//!
//! The component is written with the Naamio Space Extension Kit for Rust. [`Turtle`]
//! describes the syntax and hands documents to the grammar, and the kit exports it through
//! the contract.
//!
//! [`grammar`] reads a document into statements; it is public so that its tests, and
//! anyone embedding the grammar natively, can run it without a component. The private
//! modules split the work: `tokens` reads tokens, `escapes` decodes escape sequences, and
//! `vocabulary` holds the IRIs Turtle's shorthands stand for.

mod escapes;
pub mod grammar;
mod tokens;
mod vocabulary;

use naamio_space_kit_rdf::prelude::*;

/// The Turtle syntax, as the kit exports it.
#[derive(Debug)]
pub struct Turtle;

impl Syntax for Turtle {
	/// Describes Turtle 1.2: its registered media type and file extension, and the RDF 1.2
	/// features it can express. Turtle has no syntax for named graphs, so it does not claim
	/// datasets; TriG is the syntax that adds them.
	fn description() -> SyntaxDescription {
		SyntaxDescription {
			name: "Turtle".to_owned(),
			version: "1.2".to_owned(),
			media_types: vec!["text/turtle".to_owned()],
			file_extensions: vec!["ttl".to_owned()],
			features: SyntaxFeatures::TRIPLE_TERMS | SyntaxFeatures::BASE_DIRECTION,
		}
	}

	/// Reads a Turtle document with [`grammar::read`].
	///
	/// The options' base is not used. The grammar writes relative references as they
	/// appear and the host resolves them against the same base, so the component never
	/// needs it.
	fn read(document: &[u8], _options: &ParseOptions) -> Reading {
		grammar::read(document)
	}
}

export_syntax!(Turtle);
