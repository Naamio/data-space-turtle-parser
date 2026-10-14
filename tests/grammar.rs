// SPDX-FileCopyrightText: 2026 Omnifi Foundation
// SPDX-License-Identifier: MPL-2.0

//! Feature tests for reading Turtle into statements, as written.
//!
//! The parser reports what a document says and leaves its meaning to the host. These tests
//! hold it to that: Internationalized Resource Identifiers (IRIs) come back in the form the
//! document wrote them, absolute, relative or prefixed, blank nodes keep the document's
//! labels or get numbers the parser assigns, and literals keep their lexical form with the
//! datatype or language the document gave. Each test names the part of the Turtle 1.2
//! grammar it covers.

use std::error::Error;

use naamio_space_kit_rdf::contract::parser::ParseError;
use naamio_space_kit_rdf::contract::terms::{
	Annotation, BaseDirection, BlankNode, IriReference, Language, Literal, Node, PrefixDeclaration,
	PrefixedName, Quad, Span, Statement, Term, Triple,
};
use turtle_parser::grammar::parse;

/// A boxed error type broad enough for anything a test below might fail on.
type TestResult = Result<(), Box<dyn Error>>;

/// The IRI Turtle's `a` stands for.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The datatype of an integer written without quotes, such as `42`.
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

/// The datatype of a number written with a decimal point and no exponent, such as `3.14`.
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";

/// The datatype of a number written with an exponent, such as `1e10`.
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

/// The datatype of `true` and `false` written without quotes.
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

/// The property linking a list node to its item.
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";

/// The property linking a list node to the rest of the list.
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";

/// The IRI that ends a list, and that an empty collection `()` stands for.
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

/// Parses `document`, discarding spans, for tests about what the statements say.
fn statements(document: &str) -> Result<Vec<Statement>, Box<dyn Error>> {
	let located = parse(document.as_bytes()).map_err(|error| format!("{error:?}"))?;
	Ok(located
		.into_iter()
		.map(|located| located.statement)
		.collect())
}

/// Parses `document` and keeps only its quads, for tests about triples.
fn quads(document: &str) -> Result<Vec<Quad>, Box<dyn Error>> {
	Ok(statements(document)?
		.into_iter()
		.filter_map(|statement| match statement {
			Statement::Quad(quad) => Some(quad),
			_ => None,
		})
		.collect())
}

/// An IRI written in angle brackets that has a scheme, which the host uses as written.
fn absolute(iri: &str) -> IriReference {
	IriReference::Absolute(iri.to_owned())
}

/// An IRI written in angle brackets without a scheme, which the host resolves.
fn relative(reference: &str) -> IriReference {
	IriReference::Relative(reference.to_owned())
}

/// A prefixed name, with its prefix and local part as the document wrote them.
fn prefixed(prefix: &str, local: &str) -> IriReference {
	IriReference::Prefixed(PrefixedName {
		prefix: prefix.to_owned(),
		local: local.to_owned(),
	})
}

/// A literal with the given lexical form and annotation.
fn literal(lexical_form: &str, annotation: Option<Annotation>) -> Term {
	Term::Literal(Literal {
		lexical_form: lexical_form.to_owned(),
		annotation,
	})
}

/// A literal whose datatype is the absolute IRI `datatype`, as the parser writes the
/// shorthand forms for numbers and booleans.
fn typed(lexical_form: &str, datatype: &str) -> Term {
	literal(lexical_form, Some(Annotation::Datatype(absolute(datatype))))
}

/// A quad in the default graph with no triple terms, the shape of every quad Turtle writes
/// outside triple terms.
const fn quad(subject: Node, predicate: IriReference, object: Term) -> Quad {
	Quad {
		subject,
		predicate,
		object,
		graph_name: None,
		triple_terms: Vec::new(),
	}
}

/// The two prefix forms, `@prefix` and SPARQL's `PREFIX`, both declare a prefix. The
/// namespace stays in the form it was written; a relative one is the host's to resolve.
#[test]
fn reads_both_forms_of_prefix_declaration() -> TestResult {
	let found = statements("@prefix ex: <https://example.org/> .\nPREFIX local: <vocabulary#>\n")?;

	assert_eq!(
		found,
		vec![
			Statement::Prefix(PrefixDeclaration {
				prefix: "ex".to_owned(),
				namespace: absolute("https://example.org/"),
			}),
			Statement::Prefix(PrefixDeclaration {
				prefix: "local".to_owned(),
				namespace: relative("vocabulary#"),
			}),
		]
	);

	Ok(())
}

/// The base forms, `@base` and `BASE`, and Turtle 1.2's `VERSION` directive are reported in
/// document order, because each applies to what follows it.
#[test]
fn reads_base_and_version_directives_in_order() -> TestResult {
	let found = statements("VERSION \"1.2\"\n@base <https://pod.example/> .\nBASE <people/>\n")?;

	assert_eq!(
		found,
		vec![
			Statement::Version("1.2".to_owned()),
			Statement::Base(absolute("https://pod.example/")),
			Statement::Base(relative("people/")),
		]
	);

	Ok(())
}

/// A triple of IRIs in angle brackets becomes one quad. The parser decides absolute or
/// relative by whether the IRI starts with a scheme, and resolves neither.
#[test]
fn reads_a_triple_of_absolute_and_relative_iris() -> TestResult {
	let found = quads("<https://example.org/alice> <knows> <../bob> .")?;

	assert_eq!(
		found,
		vec![quad(
			Node::Iri(absolute("https://example.org/alice")),
			relative("knows"),
			Term::Iri(relative("../bob")),
		)]
	);

	Ok(())
}

/// Prefixed names keep their prefix and local part, including the empty default prefix.
/// Escapes in a local name, such as `\-`, are decoded, so the host sees the name the author
/// meant.
#[test]
fn reads_prefixed_names_with_their_prefixes_and_decoded_local_parts() -> TestResult {
	let found = quads(":alice ex:first\\-name \"Alice\" .")?;

	assert_eq!(
		found,
		vec![quad(
			Node::Iri(prefixed("", "alice")),
			prefixed("ex", "first-name"),
			literal("Alice", None),
		)]
	);

	Ok(())
}

/// `a` in the predicate position is shorthand for `rdf:type`, written out in full because
/// the document need not have declared `rdf:`.
#[test]
fn reads_a_as_rdf_type() -> TestResult {
	let found = quads("<alice> a <Person> .")?;

	assert_eq!(found[0].predicate, absolute(RDF_TYPE));

	Ok(())
}

/// A semicolon repeats the subject with a new predicate, and a comma repeats the subject and
/// predicate with a new object. Each object becomes a quad of its own.
#[test]
fn expands_predicate_and_object_lists() -> TestResult {
	let found = quads("<alice> <name> \"Alice\", \"Ally\" ; <age> 42 .")?;

	assert_eq!(
		found,
		vec![
			quad(
				Node::Iri(relative("alice")),
				relative("name"),
				literal("Alice", None)
			),
			quad(
				Node::Iri(relative("alice")),
				relative("name"),
				literal("Ally", None)
			),
			quad(
				Node::Iri(relative("alice")),
				relative("age"),
				typed("42", XSD_INTEGER)
			),
		]
	);

	Ok(())
}

/// A blank node label is passed on as written. Whether two labels are the same node is the
/// host's decision, scoped to the document.
#[test]
fn passes_on_blank_node_labels_as_written() -> TestResult {
	let found = quads("_:someone <knows> _:other .")?;

	assert_eq!(
		found,
		vec![quad(
			Node::BlankNode(BlankNode::Written("someone".to_owned())),
			relative("knows"),
			Term::BlankNode(BlankNode::Written("other".to_owned())),
		)]
	);

	Ok(())
}

/// `[ ]` and a property list in square brackets name no label, so the parser numbers the
/// nodes itself, in the order it creates them, and writes the property list's triples
/// about the numbered node.
#[test]
fn numbers_the_blank_nodes_it_creates() -> TestResult {
	let found = quads("<alice> <knows> [ <name> \"Bob\" ] .\n[] <seen> \"yes\" .")?;

	assert_eq!(
		found,
		vec![
			quad(
				Node::BlankNode(BlankNode::Generated(0)),
				relative("name"),
				literal("Bob", None)
			),
			quad(
				Node::Iri(relative("alice")),
				relative("knows"),
				Term::BlankNode(BlankNode::Generated(0)),
			),
			quad(
				Node::BlankNode(BlankNode::Generated(1)),
				relative("seen"),
				literal("yes", None)
			),
		]
	);

	Ok(())
}

/// A collection is written out as the list RDF defines: one numbered node per item, linked
/// by `rdf:first` and `rdf:rest`, and ending in `rdf:nil`. An empty collection is `rdf:nil`
/// itself.
#[test]
fn writes_collections_as_rdf_lists() -> TestResult {
	let found = quads("<list> <items> ( 1 2 ) .\n<empty> <items> () .")?;

	let first = || Node::BlankNode(BlankNode::Generated(0));
	let second = || Node::BlankNode(BlankNode::Generated(1));
	assert_eq!(
		found,
		vec![
			quad(first(), absolute(RDF_FIRST), typed("1", XSD_INTEGER)),
			quad(
				first(),
				absolute(RDF_REST),
				Term::BlankNode(BlankNode::Generated(1))
			),
			quad(second(), absolute(RDF_FIRST), typed("2", XSD_INTEGER)),
			quad(second(), absolute(RDF_REST), Term::Iri(absolute(RDF_NIL))),
			quad(
				Node::Iri(relative("list")),
				relative("items"),
				Term::BlankNode(BlankNode::Generated(0)),
			),
			quad(
				Node::Iri(relative("empty")),
				relative("items"),
				Term::Iri(absolute(RDF_NIL))
			),
		]
	);

	Ok(())
}

/// Turtle has four ways to quote a string. Each one's escape sequences are decoded, and the
/// long forms may run over several lines.
#[test]
fn reads_every_string_form_and_decodes_its_escapes() -> TestResult {
	let document =
		"<s> <p> \"tab\\there\", 'it\\'s', \"\"\"two\nlines\"\"\", '''\\u00e9\\U0001F600''' .";

	let objects: Vec<Term> = quads(document)?
		.into_iter()
		.map(|quad| quad.object)
		.collect();

	assert_eq!(
		objects,
		vec![
			literal("tab\there", None),
			literal("it's", None),
			literal("two\nlines", None),
			literal("é😀", None),
		]
	);

	Ok(())
}

/// A language tag follows the string, and RDF 1.2 lets it carry a base direction after two
/// hyphens, as in `@ar--rtl`.
#[test]
fn reads_language_tags_with_and_without_a_direction() -> TestResult {
	let objects: Vec<Term> = quads("<s> <p> \"Hei\"@fi, \"مرحبا\"@ar--rtl .")?
		.into_iter()
		.map(|quad| quad.object)
		.collect();

	assert_eq!(
		objects,
		vec![
			literal(
				"Hei",
				Some(Annotation::Language(Language {
					tag: "fi".to_owned(),
					direction: None,
				})),
			),
			literal(
				"مرحبا",
				Some(Annotation::Language(Language {
					tag: "ar".to_owned(),
					direction: Some(BaseDirection::Rtl),
				})),
			),
		]
	);

	Ok(())
}

/// A datatype after `^^` is an IRI reference like any other: prefixed or in angle brackets,
/// and passed on unexpanded.
#[test]
fn reads_datatypes_as_iri_references() -> TestResult {
	let objects: Vec<Term> =
		quads("<s> <p> \"42\"^^xsd:integer, \"x\"^^<https://example.org/type> .")?
			.into_iter()
			.map(|quad| quad.object)
			.collect();

	assert_eq!(
		objects,
		vec![
			literal("42", Some(Annotation::Datatype(prefixed("xsd", "integer")))),
			literal(
				"x",
				Some(Annotation::Datatype(absolute("https://example.org/type")))
			),
		]
	);

	Ok(())
}

/// Numbers and booleans written without quotes are literals whose datatype the grammar
/// implies: integer, decimal or double by the form of the number, and boolean for `true` and
/// `false`.
#[test]
fn reads_numbers_and_booleans_with_their_implied_datatypes() -> TestResult {
	let objects: Vec<Term> = quads("<s> <p> 42, -3.14, 1.5e10, true, false .")?
		.into_iter()
		.map(|quad| quad.object)
		.collect();

	assert_eq!(
		objects,
		vec![
			typed("42", XSD_INTEGER),
			typed("-3.14", XSD_DECIMAL),
			typed("1.5e10", XSD_DOUBLE),
			typed("true", XSD_BOOLEAN),
			typed("false", XSD_BOOLEAN),
		]
	);

	Ok(())
}

/// An RDF 1.2 triple term, `<<( s p o )>>`, may appear as an object. The parser puts its
/// triple in the quad's list and refers to it by position, because contract types cannot
/// contain themselves. A triple term inside a triple term refers to an earlier entry.
#[test]
fn reads_nested_triple_terms_into_the_quads_list() -> TestResult {
	let found = quads("<claim> <reifies> <<( <bob> <says> <<( <alice> <age> 42 )>> )>> .")?;

	assert_eq!(
		found,
		vec![Quad {
			subject: Node::Iri(relative("claim")),
			predicate: relative("reifies"),
			object: Term::TripleTerm(1),
			graph_name: None,
			triple_terms: vec![
				Triple {
					subject: Node::Iri(relative("alice")),
					predicate: relative("age"),
					object: typed("42", XSD_INTEGER),
				},
				Triple {
					subject: Node::Iri(relative("bob")),
					predicate: relative("says"),
					object: Term::TripleTerm(0),
				},
			],
		}]
	);

	Ok(())
}

/// Comments run from `#` to the end of the line and say nothing, so they produce no
/// statements. A `#` inside an IRI or a string is part of it, not a comment.
#[test]
fn ignores_comments_but_not_hashes_inside_terms() -> TestResult {
	let found = quads("# a comment\n<s> <p#q> \"#not a comment\" . # another\n")?;

	assert_eq!(
		found,
		vec![quad(
			Node::Iri(relative("s")),
			relative("p#q"),
			literal("#not a comment", None)
		)]
	);

	Ok(())
}

/// A document with nothing but whitespace and comments is valid and states nothing.
#[test]
fn reads_an_empty_document_as_no_statements() -> TestResult {
	assert_eq!(statements("  \n# nothing here\n")?, Vec::new());

	Ok(())
}

/// Each quad's span is the term that completed it, as the contract specifies. In
/// `<s> <p> <a>, <b> .`, the second quad points at `<b>`, so an error the host finds in it
/// points at the right object.
#[test]
fn gives_each_quad_the_span_of_the_term_that_completed_it() -> TestResult {
	let document = "<s> <p> <a>, <b> .";

	let located = parse(document.as_bytes()).map_err(|error| format!("{error:?}"))?;

	let spans: Vec<Span> = located.iter().map(|located| located.span).collect();
	assert_eq!(
		spans,
		vec![Span { start: 8, end: 11 }, Span { start: 13, end: 16 }]
	);

	Ok(())
}

/// A document that breaks the grammar is refused with a syntax error whose span points at
/// the problem, so a person can find it.
#[test]
fn refuses_a_document_that_breaks_the_grammar() -> TestResult {
	let cases = [
		("a triple without its full stop", "<s> <p> <o>", 11),
		("a string that is never closed", "<s> <p> \"open .", 8),
		("a literal as a subject", "\"Alice\" <p> <o> .", 0),
		("an IRI with a space in it", "<s> <p> <a b> .", 8),
	];

	for (case, document, expected_start) in cases {
		let outcome = parse(document.as_bytes());

		let Err(ParseError::Syntax(error)) = outcome else {
			return Err(format!(
				"{case} should be a syntax error, but the outcome was {outcome:?}"
			)
			.into());
		};
		assert_eq!(
			error.span.start, expected_start,
			"{case}: {}",
			error.message
		);
	}

	Ok(())
}

/// Reified triples, `<< s p o >>`, and annotations, `{| … |}`, are RDF 1.2 Turtle forms this
/// version of the parser does not read yet. It says so, rather than misreading them.
#[test]
fn reports_reified_triples_and_annotations_as_unsupported() {
	for document in ["<< <s> <p> <o> >> <q> <r> .", "<s> <p> <o> {| <q> <r> |} ."] {
		let outcome = parse(document.as_bytes());

		assert!(
			matches!(outcome, Err(ParseError::Unsupported(_))),
			"{document} should be unsupported, but the outcome was {outcome:?}"
		);
	}
}
