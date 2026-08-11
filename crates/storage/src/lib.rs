mod agent_store;
pub mod agent_types;
pub mod board;
pub mod board_positions;
pub mod board_projection;
pub mod board_properties;
pub mod board_templates;
pub mod folder_import;
pub mod home;
pub mod note_links;
pub mod onboarding;
pub mod reminders;
pub mod search;
pub mod trash;
pub mod workspace;
pub mod workspace_links;

pub use agent_store::{ChangeDomain, Store, StoreOptions};

#[cfg(test)]
mod test_alloc;
