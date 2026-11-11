//! The heart of Task Rail, and deliberately the most boring crate in the
//! workspace.
//!
//! Nothing in here talks to a disk, a network, a window or a clock. The
//! domain types describe what a rail holds, the [`ports`] describe the seams
//! the rest of the app plugs into, and the [`usecases`] turn a pile of items
//! into exactly what the rail should draw. Because of that, every rule about
//! the fold, ordering and calendar blending is unit-tested right here with no
//! setup at all.
//!
//! If you find yourself wanting to add an HTTP call or a SQL query to this
//! crate, that is the signal to add an adapter crate instead and reach it
//! through a port.

pub mod autohide;
pub mod domain;
pub mod memory;
pub mod ports;
pub mod services;
pub mod settings;
pub mod usecases;
