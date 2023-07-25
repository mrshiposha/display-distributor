use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum ServerMessage {
    LeaseGranted,
    LeaseRevoked,
    LeaseNotFound,
    NoPermission,
}

#[derive(Serialize, Deserialize)]
pub enum ClientMessage {
    RequestDisplays,
    ReleaseDisplays,
}
