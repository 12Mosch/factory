use super::*;

#[derive(Default)]
pub(in crate::simulation) struct StableHasher {
    hash: u64,
    initialized: bool,
}

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.hash
    }

    fn write(&mut self, bytes: &[u8]) {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;

        if !self.initialized {
            self.hash = FNV_OFFSET;
            self.initialized = true;
        }

        for byte in bytes {
            self.hash ^= u64::from(*byte);
            self.hash = self.hash.wrapping_mul(FNV_PRIME);
        }
    }
}

pub(in crate::simulation) fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

pub(in crate::simulation) fn hash_world(seed: u64, x: WorldTileCoord, y: WorldTileCoord) -> u64 {
    let x_bits = x as u64;
    let y_bits = y as u64;
    splitmix64(seed ^ x_bits.rotate_left(32) ^ y_bits.rotate_left(1))
}
