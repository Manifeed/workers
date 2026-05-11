use std::io::Cursor;

use crate::error::{Result, RssWorkerError};

pub fn parse_feed_bytes(bytes: &[u8], feed_url: &str) -> Result<feed_rs::model::Feed> {
    feed_rs::parser::parse(Cursor::new(bytes)).map_err(|error| {
        RssWorkerError::InvalidPayload(format!("Failed to parse RSS feed {feed_url}: {error}"))
    })
}
