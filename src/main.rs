use clap::Parser;
use distributor::SeatId;
use log::{error, info};
use thiserror::Error;

use crate::distributor::Distributor;

mod dbus;
mod distributor;
mod drm;
mod logging;

#[derive(Error, Debug)]
pub enum Error {
    #[error("DBus connection lost")]
    DBusLost,

    #[error("Unable to parse a drm's connector `<interface>-<id>`")]
    UnableToParseDisplayId,

    #[error("The current session is not bind to a seat")]
    NoSeat,

    #[error("Unable to discover a peer PID")]
    NoPeerPid,

    #[error("Invalid message from a peer")]
    PeerBadMsg,

    #[error("Seat \"{0}\" is busy")]
    SeatBusy(SeatId),

    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Env error: {0}")]
    Env(#[from] std::env::VarError),

    #[error("DBus error: {0}")]
    DBus(#[from] ::dbus::Error),
}

#[derive(Parser)]
struct Cli {
    #[arg(short, long, default_value_t = log::LevelFilter::Info)]
    log_level: log::LevelFilter,
}

fn main() {
    let cli = Cli::parse();
    logging::setup(cli.log_level).expect("Couldn't setup logging");

    if let Err(err) = run() {
        error!("{err}");
    }
}

fn run() -> Result<(), Error> {
    info!("The {} is started", env!("CARGO_PKG_NAME"));

    let mut distributor = Distributor::new()?;
    distributor.listen_clients()?;

    Ok(())
}
