use std::error::Error;

use serde::Serialize;

pub struct Config<S = BincodeSerder>
where
    S: Serder,
{
    pub(crate) serializer: Option<S>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            serializer: Some(BincodeSerder),
        }
    }
}

impl<S: Serder> Config<S> {
    pub fn with_serializer(mut self, serializer: Option<S>) -> Self {
        self.serializer = serializer;
        self
    }
    pub(crate) fn serialize<T: Serialize>(&self, data: &T) -> Result<Vec<u8>, Box<dyn Error>> {
        match &self.serializer {
            Some(_) => S::serialize(data),
            None => unsafe {
                let data = data as *const _ as *const u8;
                let data = std::slice::from_raw_parts(data, std::mem::size_of::<T>());
                Ok(data.to_vec())
            },
        }
    }
    pub(crate) fn deserialize<'a, T>(&self, data: &'a [u8]) -> Result<T, Box<dyn Error>>
    where
        T: serde::de::Deserialize<'a>,
    {
        match &self.serializer {
            Some(_) => S::deserialize(data),
            None => unsafe {
                let data = data as *const _ as *const T;
                let data = std::ptr::read(data);
                Ok(data)
            },
        }
    }
}

pub trait Serder {
    fn serialize<T>(data: &T) -> Result<Vec<u8>, Box<dyn Error>>
    where
        T: ?Sized + serde::Serialize;
    fn deserialize<'a, T>(data: &'a [u8]) -> Result<T, Box<dyn Error>>
    where
        T: serde::de::Deserialize<'a>;
}

pub struct BincodeSerder;

impl Serder for BincodeSerder {
    fn serialize<T>(data: &T) -> Result<Vec<u8>, Box<dyn Error>>
    where
        T: ?Sized + serde::Serialize,
    {
        bincode::serialize(data).map_err(|e| e.into())
    }
    fn deserialize<'a, T>(data: &'a [u8]) -> Result<T, Box<dyn Error>>
    where
        T: serde::de::Deserialize<'a>,
    {
        bincode::deserialize(data).map_err(|e| e.into())
    }
}
