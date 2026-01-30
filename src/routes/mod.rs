pub mod getsorcery;
pub mod open;
pub mod passthrough;
pub mod templates;
pub mod wellknown;

pub use getsorcery::{
    chrome_redirect_handler, editors_handler as getsorcery_editors,
    frameworks_handler as getsorcery_frameworks, install_script_handler,
    landing_handler as getsorcery_landing, platforms_handler as getsorcery_platforms,
};
pub use open::open_handler;
pub use passthrough::{catchall_handler, root_handler, MirrorQuery, PassthroughQuery};
pub use wellknown::wellknown_handler;
