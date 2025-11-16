use std::pin::Pin;

use anyhow::{bail, Result};
use futures::Stream;

pub type ChatStream = Pin<Box<dyn Stream<Item = Result<String>> + Send>>;

pub fn streaming_not_supported() -> Result<ChatStream> {
    bail!("streaming not implemented yet")
}
