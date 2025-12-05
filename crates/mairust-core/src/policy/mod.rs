//! Policy Engine Module
//!
//! Evaluates policy rules against messages to determine actions for
//! inbound and outbound email processing.

mod engine;

pub use engine::{
    PolicyContext, PolicyEngine, PolicyEvaluation, PolicyEvaluationResult, PolicyMatch,
};
