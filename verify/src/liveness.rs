// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Liveness challenge: during the photo a random gesture is requested ("show 3 fingers") and
//! checked with MediaPipe Hands' 21 points. An old photo cannot answer a number
//! chosen at that moment.

use rand::Rng;

/// Number of fingers to show (1–5), chosen at random on the device.
#[uniffi::export]
pub fn new_finger_challenge() -> u8 {
    rand::thread_rng().gen_range(1..=5)
}

/// Counts extended fingers. Input: 21 points × [x, y, z] (63 numbers) from MediaPipe Hands.
#[uniffi::export]
pub fn count_fingers(lm: Vec<f32>) -> u8 {
    if lm.len() != 63 || lm.iter().any(|v| !v.is_finite()) {
        return 0;
    }
    let y = |i: usize| lm[i * 3 + 1];
    let x = |i: usize| lm[i * 3];
    // Index, middle, ring, little: the tip is above (lower y) the PIP joint
    let mut n = [(8, 6), (12, 10), (16, 14), (20, 18)].iter().filter(|&&(tip, pip)| y(tip) < y(pip)).count() as u8;
    // Thumb: the tip moves horizontally away from the index base more than the IP joint
    if (x(4) - x(5)).abs() > (x(3) - x(5)).abs() {
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand with the given fingers extended (and up = lower y).
    fn hand(extended: [bool; 5]) -> Vec<f32> {
        let mut lm = vec![0.5f32; 63];
        let set = |lm: &mut Vec<f32>, i: usize, x: f32, y: f32| {
            lm[i * 3] = x;
            lm[i * 3 + 1] = y;
        };
        set(&mut lm, 5, 0.5, 0.6); // index base
        set(&mut lm, 3, 0.45, 0.6); // thumb IP
        set(&mut lm, 4, if extended[0] { 0.3 } else { 0.48 }, 0.6);
        for (k, (tip, pip)) in [(8, 6), (12, 10), (16, 14), (20, 18)].into_iter().enumerate() {
            set(&mut lm, pip, 0.5, 0.5);
            set(&mut lm, tip, 0.5, if extended[k + 1] { 0.3 } else { 0.55 });
        }
        lm
    }

    #[test]
    fn counts_fingers_and_challenge_is_in_range() {
        assert_eq!(count_fingers(hand([true, true, true, true, true])), 5);
        assert_eq!(count_fingers(hand([false, true, true, false, false])), 2);
        assert_eq!(count_fingers(hand([false; 5])), 0);
        assert_eq!(count_fingers(vec![f32::NAN; 63]), 0);
        assert!((0..100).map(|_| new_finger_challenge()).all(|n| (1..=5).contains(&n)));
    }
}
