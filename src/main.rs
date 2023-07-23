use std::sync::Arc;

use clap::Parser;
use ::dbus::blocking::Connection;
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

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

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

    if let Err(err) = run(cli) {
        error!("{err}");
    }
}

fn run(cli: Cli) -> Result<(), Error> {
    info!("The {} is started", env!("CARGO_PKG_NAME"));

    let dbus = Connection::new_system()?;
    let mut distributor = Distributor::new(&dbus)?;
    distributor.listen_clients();

    Ok(())
}
