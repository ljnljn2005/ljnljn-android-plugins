//! Shared native readers for the iOS and Android artifact plugins.
//!
//! The plugins never execute iLEAPP/ALEAPP or any other Python tool. They open
//! the extraction directly, materialize one database at a time into a private
//! temporary directory, and run fixed read-only SQL. Evidence is never written
//! and no SQL text from the evidence is executed.

pub mod artifact;
pub mod sqlite;
pub mod time;

pub use artifact::{
    run_evidence, run_files, ArtifactFile, ArtifactRecord, SqlSpec, SqlUnit, SqlVariant,
};
pub use time::TimeKind;
