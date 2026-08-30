//! StaticDrop core: everything that is a pure function of the content tree,
//! plus rendering and the author-side network fetches (embeds, outbound link
//! checks). Design canon: staticdrop.md.
//!
//! The boundary rule: no web-framework types in this crate. crates/serve wraps
//! these modules in axum; crates/build will walk the same URL space and emit
//! files. Because both link this exact crate, static output cannot diverge
//! from the preview server.

pub mod assets;
pub mod content;
pub mod embed;
pub mod entry;
pub mod grade;
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
pub mod url;
