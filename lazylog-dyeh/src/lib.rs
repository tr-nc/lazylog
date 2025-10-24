// lazylog-dyeh - DYEH log provider for lazylog (internal)
//
// This crate provides a LogProvider implementation for DYEH/DouyinAR logs.

mod file_finder;
mod parser;
mod provider;

pub use provider::DyehLogProvider;

// Also need to copy metadata module
pub(crate) mod metadata;
