pub mod open;
pub mod passthrough;
pub mod templates;
pub mod wellknown;

pub use open::open_handler;
pub use passthrough::{root_handler, catchall_handler, PassthroughQuery, MirrorQuery};
pub use wellknown::wellknown_handler;
