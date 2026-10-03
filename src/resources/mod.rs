//! The Team API sub-clients, returned by the methods of [`Lettermint`](crate::Lettermint).
//!
//! Each sub-client holds a clone of the client (no message state, no extra connection pool) and
//! uses the team token. Their `Debug` output shows no credentials.

mod domains;
mod messages;
mod projects;
mod routes;
mod team;
mod webhooks;

pub use domains::Domains;
pub use messages::Messages;
pub use projects::{Projects, ReportForwarding};
pub use routes::Routes;
pub use team::{Stats, Suppressions, Team, TeamMembers};
pub use webhooks::{WebhookDeliveries, Webhooks};

/// Implements `Debug` without fields, so that a sub-client never shows the client's tokens.
macro_rules! opaque_debug {
    ($($name:ident),+) => {
        $(
            impl std::fmt::Debug for $name {
                fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    formatter.write_str(stringify!($name))
                }
            }
        )+
    };
}

pub(crate) use opaque_debug;
