use crate::error::DnsexError;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use tar::{Archive, Builder};
use tokio::task;

pub async fn encode_dir(src: impl AsRef<Path>) -> Result<Vec<u8>, DnsexError> {
    let src: PathBuf = src.as_ref().to_path_buf();

    task::spawn_blocking(move || -> Result<Vec<u8>, DnsexError> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = Builder::new(encoder);

        builder.append_dir_all(&src, &src)?;

        let encoder = builder.into_inner()?;
        let archive_bytes = encoder.finish()?;

        Ok(archive_bytes)
    })
    .await?
}

pub async fn decode_dir(dst: impl AsRef<Path>, data: Vec<u8>) -> Result<(), DnsexError> {
    let dst: PathBuf = dst.as_ref().to_path_buf();

    task::spawn_blocking(move || -> Result<(), DnsexError> {
        let cursor = Cursor::new(data);
        let decoder = GzDecoder::new(cursor);
        let mut archive = Archive::new(decoder);

        archive.unpack(&dst)?;

        Ok(())
    })
    .await?
}
