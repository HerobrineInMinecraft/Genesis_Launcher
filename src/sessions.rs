use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionKind {
    World,
    Server,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentSession {
    pub kind: SessionKind,
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub version: Option<String>,
    pub last_played: Option<u64>,
}

pub fn discover_recent_sessions(game_directory: &Path) -> Vec<RecentSession> {
    let mut sessions = discover_worlds(game_directory);
    sessions.extend(discover_servers(game_directory));
    sessions.sort_by(|left, right| {
        right
            .last_played
            .cmp(&left.last_played)
            .then_with(|| {
                let rank = |kind| match kind {
                    SessionKind::World => 0,
                    SessionKind::Server => 1,
                };
                rank(left.kind).cmp(&rank(right.kind))
            })
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    sessions
}

fn discover_worlds(game_directory: &Path) -> Vec<RecentSession> {
    let Ok(entries) = fs::read_dir(game_directory.join("saves")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let id = path.file_name()?.to_string_lossy().to_string();
            let level_dat = path.join("level.dat");
            let nbt = read_nbt(&level_dat).ok();
            let data = nbt
                .as_ref()
                .and_then(Tag::compound)
                .and_then(|root| root.get("Data"))
                .and_then(Tag::compound);
            let name = data
                .and_then(|data| data.get("LevelName"))
                .and_then(Tag::string)
                .map(clean_name)
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| id.clone());
            let last_played = data
                .and_then(|data| data.get("LastPlayed"))
                .and_then(Tag::long)
                .filter(|value| *value > 0)
                .map(|milliseconds| (milliseconds as u64) / 1_000)
                .or_else(|| modified_seconds(&level_dat))
                .or_else(|| modified_seconds(&path));
            let version = data
                .and_then(|data| data.get("Version"))
                .and_then(Tag::compound)
                .and_then(|version| version.get("Name"))
                .and_then(Tag::string)
                .map(clean_name)
                .filter(|version| !version.is_empty());
            Some(RecentSession {
                kind: SessionKind::World,
                id,
                name,
                address: None,
                version,
                last_played,
            })
        })
        .collect()
}

fn discover_servers(game_directory: &Path) -> Vec<RecentSession> {
    let path = game_directory.join("servers.dat");
    let Ok(root) = read_nbt(&path) else {
        return Vec::new();
    };
    let Some(servers) = root
        .compound()
        .and_then(|root| root.get("servers"))
        .and_then(Tag::list)
    else {
        return Vec::new();
    };
    let recency = server_log_recency(game_directory);
    servers
        .iter()
        .filter_map(|server| {
            let server = server.compound()?;
            let address = server
                .get("ip")
                .and_then(Tag::string)
                .map(clean_name)
                .filter(|value| !value.is_empty())?;
            let name = server
                .get("name")
                .and_then(Tag::string)
                .map(clean_name)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| address.clone());
            let played_at = recency
                .iter()
                .find(|(logged, _)| server_addresses_match(logged, &address))
                .map(|(_, played_at)| *played_at);
            Some(RecentSession {
                kind: SessionKind::Server,
                id: address.clone(),
                name,
                address: Some(address),
                version: None,
                last_played: played_at,
            })
        })
        .collect()
}

fn server_log_recency(game_directory: &Path) -> HashMap<String, u64> {
    let path = game_directory.join("logs").join("latest.log");
    let Ok(contents) = fs::read_to_string(&path) else {
        return HashMap::new();
    };
    let log_time = modified_seconds(&path).unwrap_or_else(unix_now);
    let mut recency = HashMap::new();
    for (offset, line) in contents.lines().rev().enumerate() {
        let Some(index) = line.find("Connecting to ") else {
            continue;
        };
        let address = line[index + "Connecting to ".len()..]
            .split([',', ' ', '\t'])
            .next()
            .unwrap_or("")
            .trim();
        if address.is_empty() {
            continue;
        }
        recency
            .entry(address.to_owned())
            .or_insert(log_time.saturating_sub(offset as u64));
    }
    recency
}

fn server_addresses_match(logged: &str, configured: &str) -> bool {
    logged == configured
        || logged
            .split_once(':')
            .map(|(host, _)| host == configured)
            .unwrap_or(false)
        || configured
            .split_once(':')
            .map(|(host, _)| host == logged)
            .unwrap_or(false)
}

fn modified_seconds(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn clean_name(value: &str) -> String {
    let mut cleaned = String::new();
    let mut formatting = false;
    for character in value.chars() {
        if formatting {
            formatting = false;
            continue;
        }
        if character == '§' {
            formatting = true;
        } else if !character.is_control() {
            cleaned.push(character);
            if cleaned.chars().count() >= 80 {
                break;
            }
        }
    }
    cleaned.trim().to_owned()
}

fn read_nbt(path: &Path) -> Result<Tag, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let decoded;
    let bytes = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzDecoder::new(bytes.as_slice());
        let mut output = Vec::new();
        decoder
            .read_to_end(&mut output)
            .map_err(|error| format!("gzip: {error}"))?;
        decoded = output;
        decoded.as_slice()
    } else {
        bytes.as_slice()
    };
    let mut cursor = Cursor::new(bytes);
    if cursor.byte()? != 10 {
        return Err("root tag is not a compound".to_owned());
    }
    let _root_name = cursor.string()?;
    cursor.payload(10, 0)
}

#[derive(Debug)]
enum Tag {
    Long(i64),
    String(String),
    List(Vec<Tag>),
    Compound(HashMap<String, Tag>),
    Other,
}

impl Tag {
    fn compound(&self) -> Option<&HashMap<String, Tag>> {
        match self {
            Self::Compound(value) => Some(value),
            _ => None,
        }
    }

    fn list(&self) -> Option<&[Tag]> {
        match self {
            Self::List(value) => Some(value),
            _ => None,
        }
    }

    fn string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    fn long(&self) -> Option<i64> {
        match self {
            Self::Long(value) => Some(*value),
            _ => None,
        }
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| "NBT length overflow".to_owned())?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| "unexpected end of NBT data".to_owned())?;
        self.position = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn short(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().expect("two bytes"),
        ))
    }

    fn int(&mut self) -> Result<i32, String> {
        Ok(i32::from_be_bytes(
            self.take(4)?.try_into().expect("four bytes"),
        ))
    }

    fn long(&mut self) -> Result<i64, String> {
        Ok(i64::from_be_bytes(
            self.take(8)?.try_into().expect("eight bytes"),
        ))
    }

    fn string(&mut self) -> Result<String, String> {
        let length = self.short()? as usize;
        String::from_utf8(self.take(length)?.to_vec())
            .map_err(|_| "NBT string is not UTF-8".to_owned())
    }

    fn payload(&mut self, kind: u8, depth: usize) -> Result<Tag, String> {
        if depth > 64 {
            return Err("NBT nesting is too deep".to_owned());
        }
        match kind {
            1 => {
                self.byte()?;
                Ok(Tag::Other)
            }
            2 => {
                self.short()?;
                Ok(Tag::Other)
            }
            3 => {
                self.int()?;
                Ok(Tag::Other)
            }
            4 => Ok(Tag::Long(self.long()?)),
            5 => {
                self.take(4)?;
                Ok(Tag::Other)
            }
            6 => {
                self.take(8)?;
                Ok(Tag::Other)
            }
            7 => {
                let length = self.length()?;
                self.take(length)?;
                Ok(Tag::Other)
            }
            8 => Ok(Tag::String(self.string()?)),
            9 => {
                let element_kind = self.byte()?;
                let length = self.length()?;
                let mut values = Vec::with_capacity(length.min(4096));
                for _ in 0..length {
                    values.push(self.payload(element_kind, depth + 1)?);
                }
                Ok(Tag::List(values))
            }
            10 => {
                let mut values = HashMap::new();
                loop {
                    let child_kind = self.byte()?;
                    if child_kind == 0 {
                        break;
                    }
                    let name = self.string()?;
                    let value = self.payload(child_kind, depth + 1)?;
                    values.insert(name, value);
                }
                Ok(Tag::Compound(values))
            }
            11 => {
                let length = self.length()?;
                self.take(
                    length
                        .checked_mul(4)
                        .ok_or_else(|| "NBT integer array overflow".to_owned())?,
                )?;
                Ok(Tag::Other)
            }
            12 => {
                let length = self.length()?;
                self.take(
                    length
                        .checked_mul(8)
                        .ok_or_else(|| "NBT long array overflow".to_owned())?,
                )?;
                Ok(Tag::Other)
            }
            other => Err(format!("unsupported NBT tag type {other}")),
        }
    }

    fn length(&mut self) -> Result<usize, String> {
        let length = self.int()?;
        if length < 0 || length as usize > self.bytes.len() {
            return Err("invalid NBT collection length".to_owned());
        }
        Ok(length as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_string(output: &mut Vec<u8>, value: &str) {
        output.extend_from_slice(&(value.len() as u16).to_be_bytes());
        output.extend_from_slice(value.as_bytes());
    }

    fn write_named(output: &mut Vec<u8>, kind: u8, name: &str) {
        output.push(kind);
        write_string(output, name);
    }

    #[test]
    fn parses_world_name_and_timestamp() {
        let mut nbt = vec![10, 0, 0, 10];
        write_string(&mut nbt, "Data");
        write_named(&mut nbt, 8, "LevelName");
        write_string(&mut nbt, "暮色生存");
        write_named(&mut nbt, 4, "LastPlayed");
        nbt.extend_from_slice(&1_777_777_777_000_i64.to_be_bytes());
        nbt.push(0);
        nbt.push(0);
        let mut cursor = Cursor::new(&nbt);
        cursor.byte().expect("root kind");
        cursor.string().expect("root name");
        let root = cursor.payload(10, 0).expect("root");
        let data = root
            .compound()
            .and_then(|root| root.get("Data"))
            .and_then(Tag::compound)
            .expect("data");
        assert_eq!(
            data.get("LevelName").and_then(Tag::string),
            Some("暮色生存")
        );
        assert_eq!(
            data.get("LastPlayed").and_then(Tag::long),
            Some(1_777_777_777_000)
        );
    }

    #[test]
    fn parses_server_list() {
        let mut nbt = vec![10, 0, 0, 9];
        write_string(&mut nbt, "servers");
        nbt.push(10);
        nbt.extend_from_slice(&1_i32.to_be_bytes());
        write_named(&mut nbt, 8, "name");
        write_string(&mut nbt, "NOVA SMP");
        write_named(&mut nbt, 8, "ip");
        write_string(&mut nbt, "play.example.org");
        nbt.push(0);
        nbt.push(0);
        let mut cursor = Cursor::new(&nbt);
        cursor.byte().expect("root kind");
        cursor.string().expect("root name");
        let root = cursor.payload(10, 0).expect("root");
        let server = root
            .compound()
            .and_then(|root| root.get("servers"))
            .and_then(Tag::list)
            .and_then(|servers| servers.first())
            .and_then(Tag::compound)
            .expect("server");
        assert_eq!(server.get("name").and_then(Tag::string), Some("NOVA SMP"));
        assert_eq!(
            server.get("ip").and_then(Tag::string),
            Some("play.example.org")
        );
    }
}
