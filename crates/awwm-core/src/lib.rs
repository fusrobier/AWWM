//! Platform-independent core logic of AWWM.
//!
//! Everything in this crate is pure Rust without any Win32 dependency so it can be unit
//! tested on synthetic data: geometry and DPI helpers, the snap engine, resize math,
//! gesture classification, automation-rule matching and the settings model.
#![forbid(unsafe_code)]

pub mod geometry;
pub mod gesture;
pub mod resize;
pub mod rules;
pub mod settings;
pub mod snap;

pub use geometry::{MonitorInfo, Point, Rect, Size};
