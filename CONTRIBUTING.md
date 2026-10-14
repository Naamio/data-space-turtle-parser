# Contributing to the Naamio Data Space Turtle Parser

Start with the [README](README.md) for what this component is and where it runs.

## Getting the source

The component is written with the Rust kit, which lives in its own repository and is
included here as a Git submodule at `kits/rust`. The kit includes the contract as a
submodule of its own, so clone recursively:

```shell
git clone --recurse-submodules git@github.com:Naamio/data-space-turtle-parser.git
```

Do not edit the kit or the contract inside a submodule. Change them in the
[kit's repository](https://github.com/Naamio/data-space-kit-rust) or the
[WASI specifications repository](https://github.com/Naamio/data-space-wasi),
then move the submodule forward here in its own merge request.

The component must not include the extension host, or anything that includes it, as a
submodule or a dependency. The host includes this component, and a dependency in the other
direction would form a cycle that recursive clones and Cargo would both follow.

## Tests first

Write the failing test, then the smallest code that passes it, then improve what is there if
improving it is worth doing.

A test documents behaviour and treats the implementation as a black box. Name it for what it
establishes, and say in its documentation why that matters, so someone changing the code
later knows what they would be breaking.

The grammar is also a Rust library, so its tests in `tests/` run natively, without a
component or a host. Its behaviour across the component boundary is tested from the host
side, in the [extensions repository](https://github.com/Naamio/data-space-extensions),
against the component built here.

## Gates

```shell
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo clippy --target wasm32-wasip3 -- -D warnings
cargo test
cargo build --release --target wasm32-wasip3
```

Clippy runs at `deny` for `all`, `pedantic`, and the lints named individually in
`[lints]`, with thresholds in `.clippy.toml`. Do not weaken a lint to make code
compile: change the code, or if the lint is genuinely wrong for a case, mark that one case
with `#[expect(...)]` and a `reason` explaining it. Use `expect` and not `allow`, so the
exemption is reported once it stops being needed.

Needing the same exemption in several places means the rule is scoped wrongly. Fix the
configuration instead of repeating the attribute.

`unsafe_code` is forbidden. The `unsafe` code the bindings generator writes reaches this
crate only through the kit's `export_syntax!` macro, and the compiler does not apply the
lint to code expanded from a macro defined in another crate. Do not call wit-bindgen from
this crate: its code would then count as this crate's own, and `forbid` would refuse it.

Every `.rs` file and every `Cargo.toml` carries an SPDX header, in the comment syntax for
that file type:

```rust
// SPDX-FileCopyrightText: 2026 Omnifi Foundation
// SPDX-License-Identifier: MPL-2.0
```

## Comments

Public items are documented with their arguments, what they return, what they panic on, and
an example where one helps. Private items are documented too; `missing_docs_in_private_items`
is denied.

Write for someone reading English as an additional language, and for someone meeting the code
for the first time. Say what something does and why it is that way. Do not explain the build
configuration in the file, and do not write about the writing.

## Names

No abbreviations: `configuration`, not `config`. `misc`, `common`, `core`, `utils`, `shared`
and `types` are refused as crate or module names, each one a place a decision got deferred
rather than made. Crates carry no `naamio` prefix. "User" is refused too: name the person by
their role, such as *account holder*, *operator* or *developer*.

## Commits and merge requests

Commit messages are a single line, in the imperative, specific enough to identify the
change. Detail belongs in the merge request.

Keep unrelated changes apart. A rename, a formatting pass, and a behaviour change are three
merge requests, because reviewing them together means reviewing none of them properly.

## Versions

Check a version before writing it into a manifest. The toolchain is a nightly release
pinned by date in `rust-toolchain.toml`, because this component targets `wasm32-wasip3`
from a nightly release, and an unpinned nightly would be a different compiler every day.
Bump it deliberately, in its own merge request, together with the extension host.
