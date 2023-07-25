use std::{
    collections::{HashMap, HashSet},
    fs::File,
    os::fd::{AsFd, BorrowedFd, RawFd},
    path::Path,
};

use drm::{
    self,
    control::{connector, lease::LesseeId, Device, DrmLeaseCreateResult, RawResourceHandle},
};
use nix::fcntl::OFlag;

use crate::{distributor::SeatId, Error};

type InterfaceId = u32;

#[derive(Hash, PartialEq, Eq)]
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
    displays: HashMap<SeatId, HashSet<DisplayId>>,
}

impl Card {
    pub fn new(node: &Path) -> Result<Self, Error> {
        Ok(Self {
            file: File::open(node)?,
            displays: Default::default(),
        })
    }

    pub fn add_seat_display(&mut self, seat: SeatId, display: DisplayId) {
        self.displays.entry(seat).or_default().insert(display);
    }

    pub fn lease_displays(&self, seat: &SeatId) -> Result<(RawFd, LesseeId), Error> {
        let displays = self.displays.get(seat).ok_or(Error::NoDisplays)?;

        let mut resources: Vec<RawResourceHandle> = vec![];
        for connector_handle in self.resource_handles()?.connectors() {
            let connector = self.get_connector(*connector_handle, true)?;

            let display_id = DisplayId(connector.interface(), connector.interface_id());
            if displays.contains(&display_id) {
                resources.push((*connector_handle).into());

                for encoder_handle in connector.encoders() {
                    let encoder = self.get_encoder(*encoder_handle)?;

                    if let Some(crtc_handle) = encoder.crtc() {
                        resources.push(crtc_handle.into());
                    }
                }
            }
        }

        let DrmLeaseCreateResult { fd, lessee_id } =
            self.create_lease(&resources, OFlag::O_CLOEXEC | OFlag::O_NONBLOCK)?;

        Ok((fd, lessee_id))
    }

    pub fn revoke_displays(&self, lessee_id: LesseeId) -> Result<(), Error> {
        self.revoke_lease(lessee_id)?;
        Ok(())
    }
}

impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.file.as_fd()
    }
}

impl drm::Device for Card {}

impl Device for Card {}
