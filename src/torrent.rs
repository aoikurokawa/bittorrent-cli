use std::{fmt, path::Path};

use anyhow::Context;
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use sha1::{Digest, Sha1};

use crate::download::{self, Downloaded};

#[derive(Debug, Clone, Deserialize)]
pub struct Torrent {
    /// The URL of the tracker
    pub announce: String,

    pub info: Info,
}

impl Torrent {
    pub async fn read(torrent: impl AsRef<Path>) -> anyhow::Result<Self> {
        let dot_torrent = tokio::fs::read(torrent)
            .await
            .context("read torrent file")?;
        let torrent: Torrent =
            serde_bencode::from_bytes(&dot_torrent).context("parse torrent file")?;

        Ok(torrent)
    }

    pub fn info_hash(&self) -> [u8; 20] {
        let info_bytes = serde_bencode::to_bytes(&self.info).expect("parse into bytes");
        let mut hasher = Sha1::new();
        hasher.update(&info_bytes);
        hasher.finalize().try_into().expect("")
    }

    pub fn length(&self) -> usize {
        match &self.info.keys {
            Keys::SingleFile { length } => *length,
            Keys::MultiFile { files } => files.iter().map(|file| file.length).sum(),
        }
    }

    pub fn print_tree(&self) {
        match &self.info.keys {
            Keys::SingleFile { .. } => {
                eprintln!("{}", self.info.name);
            }
            Keys::MultiFile { files } => {
                for file in files {
                    eprintln!("{:?}", file.path.join(std::path::MAIN_SEPARATOR_STR));
                }
            }
        }
    }

    pub async fn donwload_all(&self) -> anyhow::Result<Downloaded> {
        download::all(self).await
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Info {
    /// The `name` key maps to a UTF-8 encoded string which is the suggested name
    /// to save the file (or directory) as.
    pub name: String,

    /// `piece length` maps to the number of bytes in each piece the file is split into.
    ///
    /// For the purposes of transfer, files are split into fixed-size pieces
    /// which are all the same length except for possibly the last one which may be truncated.
    #[serde(rename = "piece length")]
    pub plength: usize,

    /// `pieces` maps to a string whose length is a multiple of 20.
    /// It is to be subdivided into strings of length 20,
    /// each of which is the SHA1 hash of the piece at the corresponding index.
    pub pieces: Hashes,

    /// There is also a key length or a key files, but not both or neither.
    #[serde(flatten)]
    pub keys: Keys,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Keys {
    SingleFile { length: usize },
    MultiFile { files: Vec<File> },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct File {
    pub length: usize,
    pub path: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hashes(pub Vec<[u8; 20]>);
struct HashesVisitor;

impl<'de> Visitor<'de> for HashesVisitor {
    type Value = Hashes;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a byte string whose length is multiple of 20")
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if v.len() % 20 != 0 {
            return Err(E::custom(format!("length is {}", v.len())));
        }

        Ok(Hashes(
            v.chunks_exact(20)
                .map(|slice_20| slice_20.try_into().expect("guaranteed to be length 20"))
                .collect(),
        ))
    }
}

impl<'de> Deserialize<'de> for Hashes {
    fn deserialize<D>(deserializer: D) -> Result<Hashes, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_bytes(HashesVisitor)
    }
}

impl Serialize for Hashes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let single_file = self.0.concat();
        serializer.serialize_bytes(&single_file)
    }
}
