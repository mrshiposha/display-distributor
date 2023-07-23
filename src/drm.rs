use std::{
    collections::HashMap,
    fs::File,
    os::fd::RawFd,
    path::{Path, PathBuf},
};

use drm::control::{connector, lease::LesseeId};

use crate::{
    distributor::{Pid, SeatId},
    Error,
};

type InterfaceId = u32;
pub struct DisplayId(connector::Interface, InterfaceId);

impl TryFrom<&str> for DisplayId {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let mut parts = value.split('-');

        let iface = parts
            .next()
            .ok_or(Error::UnableToParseDisplayId)?
            .as_bytes()
            .into();

        let iface_id = parts
            .next()
            .ok_or(Error::UnableToParseDisplayId)?
            .parse::<u32>()
            .map_err(|_| Error::UnableToParseDisplayId)?;

        Ok(Self(iface, iface_id))
    }
}

pub struct Card {
    file: File,
    displays: HashMap<SeatId, Vec<DisplayId>>,
}

impl Card {
    pub fn new(node: &Path) -> Result<Self, Error> {
        Ok(Self {
            file: File::open(node)?,
            displays: Default::default(),
        })
    }

    pub fn add_seat_display(&mut self, seat: SeatId, display: DisplayId) {
        self.displays.entry(seat).or_default().push(display);
    }

    pub fn lease_displays(&self, seat: SeatId) -> Result<(RawFd, LesseeId), Error> {
        todo!()
    }
}
