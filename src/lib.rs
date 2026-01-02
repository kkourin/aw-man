#![allow(unused)]
#![allow(clippy::missing_panics_doc)]

#[macro_use]
extern crate tracing;

#[cfg(feature = "benchmarking")]
// This is only for benchmarking
pub mod natsort;

#[cfg(feature = "benchmarking")]
mod com;

#[cfg(feature = "benchmarking")]
#[allow(unused)]
pub mod resample;

pub mod graphql;