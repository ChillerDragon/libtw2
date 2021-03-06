use StatsBrowserCb;
use arrayvec::ArrayString;
use addr::ProtocolVersion;
use addr::ServerAddr;
use csv;
use ipnet::Ipv4Net;
use serverbrowse::protocol::ClientInfo;
use serverbrowse::protocol::IpAddr;
use serverbrowse::protocol::ServerInfo;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fs::File;
use std::fs;
use std::io::BufWriter;
use std::io::Write;
use std::mem;
use std::process;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::time::Instant;

mod json {
    use addr;
    use arrayvec::Array;
    use arrayvec::ArrayString;
    use serverbrowse::protocol;
    use std::convert::TryFrom;
    use std::convert::TryInto;
    use std::fmt::Write;
    use std::str;

    #[derive(Eq, Ord, PartialEq, PartialOrd)]
    pub struct Addr(pub addr::ServerAddr);

    #[derive(Serialize)]
    pub struct MasterInfo<'a> {
        pub servers: &'a [Server<'a>],
    }
    #[derive(Serialize)]
    pub struct Server<'a> {
        pub addresses: Vec<Addr>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub location: Option<ArrayString<[u8; 15]>>,
        pub info: &'a ServerInfo,
    }
    #[derive(Serialize)]
    pub struct ServerInfo {
        pub max_clients: i32,
        pub max_players: i32,
        pub passworded: bool,
        pub game_type: ArrayString<[u8; 16]>,
        pub name: ArrayString<[u8; 64]>,
        pub map: MapInfo,
        pub version: ArrayString<[u8; 32]>,
        pub clients: Vec<ClientInfo>,
    }
    #[derive(Serialize)]
    pub struct MapInfo {
        pub name: ArrayString<[u8; 32]>,
    }
    #[derive(Serialize)]
    pub struct ClientInfo {
        pub name: ArrayString<[u8; 16]>,
        pub clan: ArrayString<[u8; 16]>,
        pub country: i32,
        pub score: i32,
        pub is_player: bool,
    }

    impl serde::Serialize for Addr {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where
            S: serde::Serializer,
        {
            let mut result: ArrayString<[u8; 64]> = ArrayString::new();
            result.push_str(match self.0.version {
                addr::ProtocolVersion::V5 => "tw-0.5+udp://",
                addr::ProtocolVersion::V6 => "tw-0.6+udp://",
                addr::ProtocolVersion::V7 => "tw-0.7+udp://",
            });
            write!(result, "{}", self.0.addr).unwrap();
            serializer.serialize_str(&result)
        }
    }

    pub struct Error;

    fn s<A: Array<Item=u8> + Copy>(bytes: &[u8]) -> Result<ArrayString<A>, Error> {
        let string = str::from_utf8(bytes).map_err(|_| Error)?;
        let mut result = ArrayString::new();
        result.try_push_str(string).map_err(|_| Error)?;
        Ok(result)
    }
    impl<'a> TryFrom<&'a super::ClientInfo> for ClientInfo {
        type Error = Error;
        fn try_from(i: &'a super::ClientInfo) -> Result<ClientInfo, Error> {
            Ok(ClientInfo {
                name: s(&i.name)?,
                clan: s(&i.clan)?,
                country: i.country,
                score: i.score,
                is_player: i.is_player != 0,
            })
        }
    }
    impl<'a> TryFrom<&'a super::ServerInfo> for ServerInfo {
        type Error = Error;
        fn try_from(i: &'a super::ServerInfo) -> Result<ServerInfo, Error> {
            let mut result = ServerInfo {
                max_clients: i.max_clients,
                max_players: i.max_players,
                passworded: i.flags & protocol::SERVERINFO_FLAG_PASSWORDED != 0,
                game_type: s(&i.game_type)?,
                name: s(&i.name)?,
                map: MapInfo {
                    name: s(&i.map)?,
                },
                version: s(&i.version)?,
                clients: Vec::new(),
            };
            result.clients.reserve_exact(i.clients.len());
            for c in &i.clients {
                result.clients.push(c.try_into()?);
            }
            Ok(result)
        }
    }
}

#[derive(Deserialize)]
struct LocationRecord {
    network: Ipv4Net,
    location: ArrayString<[u8; 15]>,
}

pub struct ServerEntry {
    location: Option<ArrayString<[u8; 15]>>,
    info: Option<json::ServerInfo>,
}

pub struct Tracker {
    filename: String,
    locations: Vec<LocationRecord>,
    servers: Arc<Mutex<HashMap<ServerAddr, ServerEntry>>>,
}

const PROTOCOL_VERSIONS_PRIORITY: &'static [ProtocolVersion] = &[
    ProtocolVersion::V5,
    ProtocolVersion::V7,
    ProtocolVersion::V6,
];

impl Tracker {
    pub fn new(filename: String, locations_filename: Option<String>) -> Tracker {
        let locations: Result<Vec<_>, _>;
        if let Some(l) = locations_filename {
            let mut locations_reader = csv::Reader::from_path(l).unwrap();
            locations = locations_reader.deserialize().collect();
        } else {
            locations = Ok(Vec::new());
        }
        Tracker {
            filename,
            locations: locations.unwrap(),
            servers: Default::default(),
        }
    }
    pub fn start(&mut self) {
        let mut tracker_thread = Tracker {
            filename: mem::replace(&mut self.filename, String::new()),
            locations: Vec::new(),
            servers: self.servers.clone(),
        };
        thread::spawn(move || tracker_thread.handle_writeout());
    }
    fn lookup_location(&self, addr: ServerAddr) -> Option<ArrayString<[u8; 15]>> {
        let ip_addr = match addr.addr.to_srvbrowse_addr().ip_address {
            IpAddr::V4(a) => a,
            IpAddr::V6(_) => return None, // sad smiley
        };
        for LocationRecord { network, location } in &self.locations {
            if network.contains(&ip_addr) {
                return Some(*location);
            }
        }
        None
    }
    fn handle_writeout(&mut self) {
        let temp_filename = format!("{}.tmp.{}", self.filename, process::id());

        thread::sleep(Duration::from_secs(15));

        let start = Instant::now();
        let mut iteration = 0;
        loop {
            {
                let servers = self.servers.lock().unwrap();
                let mut addresses: Vec<_> = servers.keys()
                    .map(|a| a.addr).collect();
                addresses.sort_unstable();
                addresses.dedup();

                let mut result = Vec::new();
                for &addr in &addresses {
                    let mut entry = None;
                    let mut addresses = Vec::new();
                    for &version in PROTOCOL_VERSIONS_PRIORITY {
                        let server_addr = ServerAddr::new(version, addr);
                        if let Some(i) = servers.get(&server_addr) {
                            addresses.push(json::Addr(server_addr));
                            entry = Some(i);
                        }
                    }
                    addresses.sort();
                    let entry = entry.unwrap();
                    if let Some(i) = &entry.info {
                        result.push(json::Server {
                            addresses,
                            location: entry.location,
                            info: i,
                        });
                    }
                }

                let master = json::MasterInfo {
                    servers: &result,
                };

                {
                    let temp_file = File::create(&temp_filename).unwrap();
                    let mut temp_file = BufWriter::new(temp_file);
                    serde_json::to_writer(&mut temp_file, &master).unwrap();
                    temp_file.flush().unwrap();
                    // Drop the temporary file.
                }

                fs::rename(&temp_filename, &self.filename).unwrap();
                // Drop the mutex.
            }
            let elapsed = start.elapsed();
            if elapsed.as_secs() <= iteration {
                let remaining_ns = 1_000_000_000 - elapsed.subsec_nanos();
                thread::sleep(Duration::new(0, remaining_ns));
                iteration += 1;
            } else {
                iteration = elapsed.as_secs();
            }
        }
    }
}

impl StatsBrowserCb for Tracker {
    fn on_server_new(&mut self, addr: ServerAddr, info: &ServerInfo) {
        let mut servers = self.servers.lock().unwrap();
        let info = json::ServerInfo::try_from(info).ok();
        assert!(servers.insert(addr, ServerEntry {
            location: self.lookup_location(addr),
            info,
        }).is_none());
    }
    fn on_server_change(
        &mut self,
        addr: ServerAddr,
        _old: &ServerInfo,
        new: &ServerInfo,
    ) {
        let mut servers = self.servers.lock().unwrap();
        servers.get_mut(&addr).unwrap().info = json::ServerInfo::try_from(new).ok();
    }
    fn on_server_remove(&mut self, addr: ServerAddr, _last: &ServerInfo) {
        let mut servers = self.servers.lock().unwrap();
        assert!(servers.remove(&addr).is_some());
    }
}
