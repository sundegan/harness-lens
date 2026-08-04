use std::path::Path;

use crate::SourceId;

pub(crate) fn source_id_for_paths(provider: &str, paths: &[&Path]) -> SourceId {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET_BASIS;
    for byte in provider.as_bytes().iter().copied() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(PRIME);
    }
    hash = hash.wrapping_mul(PRIME);
    for path in paths {
        for byte in path.to_string_lossy().bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(PRIME);
        }
        hash = hash.wrapping_mul(PRIME);
    }

    SourceId::new(format!("{provider}:{hash:016x}"))
}
