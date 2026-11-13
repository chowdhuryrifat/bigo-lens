//! BigO Lens Core Analysis Library
//!
//! Pure Rust domain logic for static complexity, workload evaluation,
//! recurrence solving, constraints, and feasibility.
//!
//! Conforms strictly to ADR 0012 and RULES.md.

#![forbid(unsafe_code)]

pub mod api;
pub mod complexity;
pub mod constraints;
pub mod estimation;
pub mod feasibility;
pub mod generator;
pub mod growth_fit;
pub mod ir;
pub mod models;
pub mod recurrence;
pub mod solvers;
pub mod statistics;
