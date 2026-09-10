//! Sajt: everything that is a pure function of the content tree, plus
//! rendering and the author-side network fetches (embeds, outbound link
//! checks). Design canon: sajt.md.
//!
//! The boundary rule: no web-framework types in the library. The binary's
//! `serve` module wraps these modules in axum; its `build` module walks the
//! same URL space and emits files. Because both call this exact code, static
//! output cannot diverge from the preview server.

pub mod assets;
pub mod config;
pub mod content;
pub mod embed;
pub mod entry;
pub mod footnotes;
pub mod grade;
pub mod highlight;
pub mod lang;
pub mod markdown;
pub mod media;
pub mod outbound;
pub mod page;
pub mod postdate;
pub mod render;
pub mod sanitize;
pub mod security;
pub mod slug;
pub mod stats;
pub mod tags;
pub mod templates;
#[cfg(test)]
mod testutil;
pub mod url;
