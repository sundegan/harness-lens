use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::{Checkpoint, ProviderInfo, Result};

pub(crate) fn decode<T>(info: &ProviderInfo, checkpoint: Option<&Checkpoint>) -> Result<T>
where
    T: Default + DeserializeOwned,
{
    let Some(checkpoint) = checkpoint else {
        return Ok(T::default());
    };
    checkpoint.decode_state(info)
}

pub(crate) fn encode(info: &ProviderInfo, state: impl Serialize) -> Result<Checkpoint> {
    Checkpoint::from_state(info, &state)
}
