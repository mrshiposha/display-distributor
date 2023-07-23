use std::time::Duration;

use crate::{distributor::SeatId, Error};
use dbus::blocking::Connection;
use log::{error, info, trace};

pub mod login1 {
    pub mod manager;
    pub mod session;
}

use login1::manager::*;
use login1::session::*;

// pub async fn connection(mut tx: Sender<Arc<SyncConnection>>) {
//     loop {
//         info!("Acquiring the DBus connection...");

//         let (resource, connection) = match dbus_tokio::connection::new_system_sync() {
//             Ok(result) => result,
//             Err(err) => {
//                 error!("Unable to recreate the DBus connection: {err}");
//                 continue;
//             }
//         };

//         if let Err(err) = tx.send(connection).await {
//             error!("Unable to recreate the DBus connection: {err}");
//         }

//         info!("Acquiring the DBus connection...DONE");

//         let err = resource.await;
//         error!("DBus connection lost: {err}");
//     }
// }

pub trait ProcessSeat {
    fn process_seat(&self, pid: u32) -> Result<SeatId, Error>;
}

impl ProcessSeat for Connection {
    fn process_seat(&self, pid: u32) -> Result<SeatId, Error> {
        let freedesktop_service = "org.freedesktop.login1";

        let timeout = Duration::from_secs(5);
        let session_manager =
            self.with_proxy(freedesktop_service, "/org/freedesktop/login1", timeout);

        trace!("Acquiring session DBus path");
        let session_path = session_manager.get_session_by_pid(pid)?;
        trace!("Session path: {session_path}");

        let session = self.with_proxy(freedesktop_service, session_path, timeout);

        let (seat_id, _) = session.seat()?;
        if seat_id.is_empty() {
            return Err(Error::NoSeat);
        }

        Ok(seat_id.into())
    }
}
