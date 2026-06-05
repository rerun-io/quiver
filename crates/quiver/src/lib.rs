#![cfg_attr(doc, doc = include_str!("../README.md"))]
// NOTE: the code blocks in the README double as doctests for this crate.
//!
//! ## Feature flags
#![cfg_attr(feature = "document-features", doc = document_features::document_features!())]

pub use quiver_core::*;

#[cfg(feature = "derive")]
pub use quiver_derive::Quiver;
