// SPDX-FileCopyrightText: 2026 Omnifi Foundation
// SPDX-License-Identifier: MPL-2.0

//! The Internationalized Resource Identifiers (IRIs) the Turtle grammar writes itself.
//!
//! Turtle's shorthands stand for IRIs the document never spells out: `a` for `rdf:type`, a
//! collection for the `rdf:first` and `rdf:rest` links of a list, and unquoted numbers and
//! booleans for their XML Schema datatypes. The parser writes these as absolute IRIs,
//! because the document need not have declared the `rdf:` or `xsd:` prefixes.

/// The property Turtle's `a` stands for.
pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The property linking a list node to its item.
pub const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";

/// The property linking a list node to the rest of the list.
pub const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";

/// The end of every list, and what an empty collection `()` stands for.
pub const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

/// The datatype of an unquoted integer, such as `42`.
pub const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

/// The datatype of an unquoted number with a decimal point and no exponent, such as `3.14`.
pub const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";

/// The datatype of an unquoted number with an exponent, such as `1e10`.
pub const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

/// The datatype of an unquoted `true` or `false`.
pub const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
