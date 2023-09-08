// This module is a fork of cfg-expr, licensed under "MIT OR Apache-2.0".
//
// The code has been simplified and adjusted to match our use cases.
//
// Source: https://github.com/EmbarkStudios/cfg-expr/tree/0.15.5
//
// See the license files included in this directory for copyright & license.

pub(crate) mod error;
pub(crate) mod expr;

#[cfg(test)]
mod tests {
    mod eval;
    mod lexer;
    mod parser;
}

pub(crate) use expr::{Expression, Predicate};
