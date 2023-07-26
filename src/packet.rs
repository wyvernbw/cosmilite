use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct Packet<T>
where
    T: Serialize + DeserializeOwned,
{
    pub dest: std::net::SocketAddr,
    data: T,
}

impl<T> Packet<T>
where
    T: Serialize + DeserializeOwned,
{
    pub fn new(data: T, dest: std::net::SocketAddr) -> Self {
        Packet { dest, data }
    }
    pub(crate) fn into_inner<S: Serializer>(self, _: &S) -> Result<PacketInner, S::SerError> {
        let data = S::serialize(&self.data)?;
        Ok(PacketInner {
            dest: self.dest,
            data,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PacketInner {
    dest: std::net::SocketAddr,
    pub data: Vec<u8>,
}

pub trait Serializer {
    type SerError: std::fmt::Debug;
    fn serialize<T: Serialize>(data: &T) -> Result<Vec<u8>, Self::SerError>;
    fn deserialize<T: DeserializeOwned>(data: &[u8]) -> Option<T>;
    fn new() -> Self;
}

impl Serializer for bincode::DefaultOptions {
    type SerError = Box<bincode::ErrorKind>;
    fn new() -> Self {
        bincode::DefaultOptions::new()
    }
    fn serialize<T: Serialize>(data: &T) -> Result<Vec<u8>, Self::SerError> {
        bincode::serialize(data)
    }
    fn deserialize<T: DeserializeOwned>(data: &[u8]) -> Option<T> {
        bincode::deserialize(data).ok()
    }
}

pub struct JsonSerializer;

impl Serializer for JsonSerializer {
    type SerError = serde_json::Error;

    fn serialize<T: Serialize>(data: &T) -> Result<Vec<u8>, Self::SerError> {
        serde_json::to_vec(data)
    }

    fn deserialize<T: DeserializeOwned>(data: &[u8]) -> Option<T> {
        serde_json::from_slice(data).ok()
    }

    fn new() -> Self {
        JsonSerializer
    }
}
