use std::{
    collections::{hash_map::Entry, HashMap},
    env, fs,
    io::{Read, Write},
    os::{
        fd::RawFd,
        unix::{
            net::{UnixListener, UnixStream},
            prelude::OsStrExt,
        },
    },
    path::{Path, PathBuf},
};

use crate::{dbus::ProcessSeat, drm::Card, Error};
use dbus::blocking::Connection;
use display_distributor::{ClientMessage, ServerMessage};
use drm::control::lease::LesseeId;
use libc::pid_t;
use log::{error, info, warn};
use sendfd::SendWithFd;
use udev::{Device, Enumerator};

pub type SeatId = String;

pub struct Distributor {
    dbus: Connection,
    cards: HashMap<PathBuf, Card>,
    leases: HashMap<SeatId, Lease>,
}

struct LeaseInfo {
    card_node: PathBuf,
    lessee_id: LesseeId,
}

struct Lease {
    pid: pid_t,
    lease_fds: Vec<RawFd>,
    infos: Vec<LeaseInfo>,
}

impl Lease {
    fn new(pid: pid_t) -> Self {
        Self {
            pid,
            lease_fds: vec![],
            infos: vec![],
        }
    }

    fn add_displays(&mut self, card_node: PathBuf, (fd, lessee_id): (RawFd, LesseeId)) {
        self.lease_fds.push(fd);
        self.infos.push(LeaseInfo {
            card_node,
            lessee_id,
        });
    }
}

impl Distributor {
    pub fn new() -> Result<Self, Error> {
        let dbus = Connection::new_system()?;
        let seat = dbus.process_seat(std::process::id())?;
        info!("Running on the Seat \"{seat}\"");

        let mut distr = Self {
            dbus,
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

        info!("Scanning graphics devices of the Seat \"{}\"...", seat);
        for dev in cards_enumerator.scan_devices()? {
            self.process_device(dev)?;
        }
        info!("Scanning graphics devices of the Seat \"{}\"...DONE", seat);

        Ok(())
    }

    fn process_device(&mut self, dev: Device) -> Result<(), Error> {
        match dev
            .devtype()
            .expect("Invalid device got matched")
            .as_bytes()
        {
            b"drm_minor" if dev.sysname().to_string_lossy().contains("card") => {
                self.get_or_add_gpu(dev)?;
            }
            b"drm_connector" => {
                if let Some(display_seat) = dev.property_value("ID_SEAT") {
                    let gpu = dev.parent().expect("Connectors always have a parent GPU");
                    let gpu_name = gpu.sysname().to_string_lossy();

                    let display_seat = display_seat.to_string_lossy().to_string();
                    let dev_name = dev.sysname().to_string_lossy().to_string();

                    let display_name = dev_name
                        .strip_prefix(&format!["{gpu_name}-"])
                        .expect("Connetcors always prefixed with the GPU name");

                    info!(
                        "Detected Seat \"{}\" connector: {}/{}",
                        display_seat, gpu_name, display_name,
                    );

                    let gpu = self.get_or_add_gpu(gpu)?;

                    let display_id = display_name.try_into()?;
                    gpu.add_seat_display(display_seat, display_id);
                }
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
            }
            Entry::Occupied(entry) => Ok(entry.into_mut()),
        }
    }

    pub fn listen_clients(&mut self) -> Result<(), Error> {
        let socketpath = env::var("DISPLAY_DISTRIBUTOR_SOCKET")?;
        let socketpath = Path::new(&socketpath);

        if socketpath.try_exists()? {
            fs::remove_file(&socketpath)?;
        }

        let listener = UnixListener::bind(socketpath)?;
        for stream in listener.incoming() {
            let result = match stream {
                Ok(stream) => self.handle_client(stream),
                Err(err) => Err((err.into(), None)),
            };

            if let Err((err, pid)) = result {
                error!(
                    "Unable to handle a client{}: {err}",
                    pid.map(|pid| format![" (pid: {pid})"]).unwrap_or_default(),
                );
                continue;
            }
        }

        Ok(())
    }

    fn handle_client(&mut self, mut stream: UnixStream) -> Result<(), (Error, Option<pid_t>)> {
        let (Some(peer_pid), ..) =
            unix_cred::get_peer_pid_ids(&stream).map_err(|e| (e.into(), None))?
        else {
            return Err((Error::NoPeerPid, None));
        };

        macro_rules! wrap_err {
            () => {
                |e| (e.into(), Some(peer_pid))
            };
            ($e:ident) => {
                |_| (Error::$e, Some(peer_pid))
            };
        }

        let peer_seat = self
            .dbus
            .process_seat(peer_pid as u32)
            .map_err(wrap_err!())?;

        let mut bytes = [0; std::mem::size_of::<ClientMessage>()];
        stream.read(&mut bytes).map_err(wrap_err!(PeerBadMsg))?;

        let message: ClientMessage = bincode::deserialize(&bytes).map_err(wrap_err!())?;
        self.handle_client_message(stream, peer_pid, peer_seat, message)
            .map_err(wrap_err!())?;

        Ok(())
    }

    fn handle_client_message(
        &mut self,
        stream: UnixStream,
        peer_pid: pid_t,
        peer_seat: SeatId,
        message: ClientMessage,
    ) -> Result<(), Error> {
        use ClientMessage::*;

        match message {
            RequestDisplays => self.handle_request_displays(stream, peer_pid, peer_seat)?,
            ReleaseDisplays => self.handle_release_displays(stream, peer_pid, peer_seat)?,
        }

        Ok(())
    }

    fn handle_request_displays(
        &mut self,
        mut stream: UnixStream,
        peer_pid: pid_t,
        peer_seat: SeatId,
    ) -> Result<(), Error> {
        if let Some(lease) = self.leases.get(&peer_seat) {
            if is_process_exist(lease.pid) {
                stream.send_msg(ServerMessage::SeatBusy)?;
                return Ok(());
            }
        }

        let mut lease = Lease::new(peer_pid);
        for (card_node, card) in self.cards.iter() {
            let displays_lease = card.lease_displays(&peer_seat)?;
            lease.add_displays(card_node.clone(), displays_lease);
        }
        stream.send_lease(lease)?;

        Ok(())
    }

    fn handle_release_displays(
        &mut self,
        mut stream: UnixStream,
        peer_pid: pid_t,
        peer_seat: SeatId,
    ) -> Result<(), Error> {
        match self.leases.entry(peer_seat) {
            Entry::Occupied(entry) => {
                if entry.get().pid == peer_pid {
                    let lease = entry.remove();
                    self.revoke_lease(stream, lease)?;
                } else {
                    stream.send_msg(ServerMessage::NoPermission)?;
                }
            }
            _ => stream.send_msg(ServerMessage::LeaseNotFound)?,
        }

        Ok(())
    }

    fn revoke_lease(&mut self, stream: UnixStream, lease: Lease) -> Result<(), Error> {
        for lease_info in lease.infos.iter() {
            let LeaseInfo {
                card_node,
                lessee_id,
            } = lease_info;

            let Some(card) = self.cards.get(card_node) else {
                warn!(
                    "A lease points to the device {} that is not found",
                    card_node.display()
                );
                continue;
            };

            if let Err(err) = card.revoke_displays(*lessee_id) {
                error!(
                    "Unable to revoke lease on the device {}: {}",
                    card_node.display(),
                    err
                );
            }
        }

        stream.send_msg_fds(ServerMessage::LeaseRevoked, &lease.lease_fds)?;

        Ok(())
    }
}

fn is_process_exist(pid: pid_t) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

trait ServerMessageSend {
    fn send_msg(&mut self, message: ServerMessage) -> Result<(), Error>;

    fn send_msg_fds(&self, message: ServerMessage, fds: &[RawFd]) -> Result<(), Error>;
}

impl ServerMessageSend for UnixStream {
    fn send_msg(&mut self, message: ServerMessage) -> Result<(), Error> {
        let encoded = bincode::serialize(&message)?;
        self.write_all(&encoded)?;

        Ok(())
    }

    fn send_msg_fds(&self, message: ServerMessage, fds: &[RawFd]) -> Result<(), Error> {
        let encoded = bincode::serialize(&message)?;
        self.send_with_fd(&encoded, fds)?;

        Ok(())
    }
}

trait LeaseSend {
    fn send_lease(&self, lease: Lease) -> Result<(), Error>;
}

impl LeaseSend for UnixStream {
    fn send_lease(&self, lease: Lease) -> Result<(), Error> {
        self.send_msg_fds(ServerMessage::LeaseGranted, &lease.lease_fds)
    }
}
