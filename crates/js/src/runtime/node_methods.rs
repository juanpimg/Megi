use super::*;

// Node method bindings (split verbatim from the former monolithic node_methods.rs).
mod attributes;
mod dialog;
mod forms;
mod mutations;
mod properties;
mod tables;
mod validation;

pub(crate) use attributes::*;
pub(crate) use dialog::*;
pub(crate) use forms::*;
pub(crate) use mutations::*;
pub(crate) use properties::*;
pub(crate) use tables::*;
pub(crate) use validation::*;
