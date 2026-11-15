// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Anti-reuse perceptual hash. The verifier reduces the photo to 9×8 gray pixels
//! (72 bytes) in memory and computes a 64-bit hash. Only the **salted** hash (XOR with
//! a secret device mask) is stored for 7 days: the image cannot be reconstructed from it.

/// Maximum distance (bits out of 64) to consider it the same photo.
pub const DUPLICATE_BITS: u32 = 6;

/// dHash: compares each pixel with its right neighbor. Input: 9×8 = 72 gray bytes.
#[uniffi::export]
pub fn dhash(gray_9x8: Vec<u8>) -> Option<u64> {
    if gray_9x8.len() != 72 {
        return None;
    }
    let mut h = 0u64;
    for row in 0..8 {
        for col in 0..8 {
            let i = row * 9 + col;
            h = (h << 1) | (gray_9x8[i] > gray_9x8[i + 1]) as u64;
        }
    }
    Some(h)
}

/// XOR with the mask preserves the Hamming distance and hides the real hash.
#[uniffi::export]
pub fn salt(hash: u64, device_mask: u64) -> u64 {
    hash ^ device_mask
}

/// Is it (almost) the same photo as a recent one?
#[uniffi::export]
pub fn is_duplicate(salted: u64, recent_salted: Vec<u64>) -> bool {
    recent_salted.iter().any(|r| (r ^ salted).count_ones() <= DUPLICATE_BITS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(shift: u8) -> Vec<u8> {
        (0..72u8).map(|i| (i % 9).wrapping_mul(20).wrapping_add(shift) ^ ((i / 9) * 3)).collect()
    }

    #[test]
    fn same_photo_with_noise_is_duplicate_other_is_not() {
        let mask = 0xdead_beef_cafe_f00d;
        let a = salt(dhash(gradient(0)).unwrap(), mask);
        let mut noisy = gradient(0);
        noisy[10] = noisy[10].wrapping_add(1); // one pixel changes slightly
        let a2 = salt(dhash(noisy).unwrap(), mask);
        let other = salt(dhash(gradient(0).into_iter().rev().collect()).unwrap(), mask);
        assert!(is_duplicate(a2, vec![a]));
        assert!(!is_duplicate(other, vec![a]));
        assert_eq!(dhash(vec![0; 10]), None);
    }
}
