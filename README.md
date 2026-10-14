# Naamio Data Space Turtle Parser

A [Naamio Data Space](https://naamio.space) extension component that parses
[Turtle](https://www.w3.org/TR/rdf12-turtle/), the text format for Resource Description
Framework (RDF) graphs. It is a WebAssembly component built for WebAssembly System
Interface (WASI) 0.3, and it runs in any host that implements the `linked-data:rdf`
contract, such as the
[Naamio Data Space extension host](https://github.com/Naamio/data-space-extensions).

## Design

The component knows Turtle and nothing more. It reads a document and streams back the
statements the document makes, as written: Internationalized Resource Identifiers (IRIs)
as they appear, absolute, relative or prefixed, blank nodes by the document's labels or by
numbers the parser assigns, and literals with the datatype or language the document gave.
It decides what Turtle's shorthands stand for, such as `a`, collections and unquoted
numbers, and leaves everything else to the host: resolving IRIs, expanding prefixes,
giving blank nodes their identity, and checking and storing every term.

The grammar is also a Rust library, so it can be tested, or embedded, without a
component.

The component is written with the
[Naamio Space Extension Kit for Rust](https://github.com/Naamio/data-space-kit-rust),
included as a Git submodule. The kit generates the bindings and exports the component, so
this repository holds only the syntax description and the grammar. The contract the
component implements is `linked-data:rdf`, from the
[Naamio Data Space WASI specifications](https://github.com/Naamio/data-space-wasi),
which the kit includes as its own submodule.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for how changes are made and what is checked before
they land.

## Licence

MPL-2.0. Copyright (c) Omnifi Foundation.
