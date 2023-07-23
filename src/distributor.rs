use std::{
    collections::{hash_map::Entry, HashMap},
    os::{fd::RawFd, unix::prelude::OsStrExt},
    path::{Path, PathBuf},
};

use crate::{dbus::ProcessSeat, drm::Card, Error};
use dbus::blocking::Connection;
use drm::control::lease::LesseeId;
use log::{error, info, trace};
use udev::{Device, Enumerator};

pub type Pid = u32;
pub type SeatId = String;

pub struct Distributor {
    cards: HashMap<PathBuf, Card>,
    leases: HashMap<SeatId, LeaseInfo>,
}

struct LeaseInfo {
    pid: Pid,
    drm_leases: Vec<(RawFd, LesseeId)>,
}

impl Distributor {
    pub fn new(dbus: &Connection) -> Result<Self, Error> {
        let seat = dbus.process_seat(std::process::id())?;
        info!("Running on the Seat \"{seat}\"");

        let mut distr = Self {
            cards: Default::default(),
            leases: Default::default(),
        };

        distr.scan_devices(seat)?;

        Ok(distr)
    }

    fn scan_devices(&mut self, seat: String) -> Result<(), Error> {
        info!("Scanning graphics devices of the Seat \"{}\"...", seat);

        let mut cards_enumerator = Enumerator::new()?;
        cards_enumerator.match_is_initialized()?;
        cards_enumerator.match_subsystem("drm")?;
        cards_enumerator.match_property("DEVTYPE", "drm_minor")?;
        cards_enumerator.match_property("DEVTYPE", "drm_connector")?;
        cards_enumerator.match_property("ID_SEAT", &seat)?;

        info!(
            "Scanning graphics devices of the Seat \"{}\"...",
            seat
        );
        for dev in cards_enumerator.scan_devices()? {
            self.process_device(&seat, dev)?;
        }
        info!(
            "Scanning graphics devices of the Seat \"{}\"...DONE",
            seat
        );

        Ok(())
    }

    fn process_device(&mut self, seat: &SeatId, dev: Device) -> Result<(), Error> {
        match dev
            .devtype()
            .expect("Invalid device got matched")
            .as_bytes()
        {
            b"drm_minor" if dev.sysname().to_string_lossy().contains("card") => {
                self.get_or_add_gpu(dev)?;
            }
            b"drm_connector" => if let Some(display_seat) = dev.property_value("ID_SEAT") {
                let gpu = dev.parent().expect("Connectors always have a parent GPU");
                let gpu_name = gpu.sysname().to_string_lossy();

                let display_seat = display_seat.to_string_lossy().to_string();
                let dev_name = dev.sysname().to_string_lossy().to_string();
                
                let display_name = dev_name.strip_prefix(&format!["{gpu_name}-"])
                    .expect("Connetcors always prefixed with the GPU name");

                info!(
                    "Detected Seat \"{}\" connector: {}/{}",
                    display_seat,
                    gpu_name,
                    display_name,
                );

                let gpu = self.get_or_add_gpu(gpu)?;

                let display_id = display_name.try_into()?;
                gpu.add_seat_display(display_seat, display_id);
            }
            _ => {}
        }

        Ok(())
    }

    fn get_or_add_gpu(&mut self, dev: Device) -> Result<&mut Card, Error> {
        let node = dev.devnode().expect("GPU must have a node");
        
        match self.cards.entry(node.to_path_buf()) {
            Entry::Vacant(entry) => {
                let dev_name = dev.sysname().to_string_lossy();
                info!("Detected GPU: {dev_name}");

                Ok(entry.insert(Card::new(node)?))
            },
            Entry::Occupied(mut entry) => Ok(entry.into_mut()),
        }
    }

    pub fn listen_clients(&mut self) {

    }
}
