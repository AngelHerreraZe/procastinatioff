// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ONNX models with tract, verified by SHA-256 BEFORE parsing them (the same bytes that were
//! verified are parsed: there is no window to swap the file in between).
//!
//! VERIFY: Spike E: choose the final models, pin their hashes here and check their inputs/outputs.
//! While a hash is not pinned, the model is not loaded and the verifier says so.

use sha2::{Digest, Sha256};
use tract_onnx::prelude::*;

/// (file, SHA-256). Same values as `scripts/models.lock`.
pub const MOVENET: (&str, &str) = ("movenet_singlepose_lightning.onnx", "PENDIENTE_SPIKE_E");
pub const DETECTOR: (&str, &str) = ("ssd_mobilenet_v1_coco.onnx", "PENDIENTE_SPIKE_E");

/// COCO ids of the TF Object Detection label map (1 = person, 84 = book).
const COCO_PERSON: i64 = 1;
const COCO_BOOK: i64 = 84;
const DETECT_MIN_SCORE: f32 = 0.4; // calibration knob

type Plan = std::sync::Arc<TypedRunnableModel>;

fn load_verified(dir: &std::path::Path, (file, sha): (&str, &str)) -> Result<Vec<u8>, String> {
    if sha.len() != 64 {
        return Err(format!("model {file} has no pinned hash (spike E)"));
    }
    let bytes = std::fs::read(dir.join(file)).map_err(|_| format!("missing model {file}"))?;
    if hex::encode(Sha256::digest(&bytes)) != sha {
        return Err(format!("tampered model: {file}"));
    }
    Ok(bytes)
}

fn plan(bytes: &[u8], input: InferenceFact) -> Result<Plan, String> {
    tract_onnx::onnx()
        .model_for_read(&mut &bytes[..])
        .and_then(|m| m.with_input_fact(0, input))
        .and_then(|m| m.into_optimized())
        .and_then(|m| m.into_runnable())
        .map_err(|e| format!("modelo no compatible: {e}"))
}

/// RGB image in RAM (the camera frame); resized by nearest neighbor.
pub struct Frame<'a> {
    pub width: usize,
    pub height: usize,
    pub rgb: &'a [u8],
}

impl Frame<'_> {
    fn sample(&self, size: usize, x: usize, y: usize, c: usize) -> u8 {
        let sx = (x * self.width / size).min(self.width - 1);
        let sy = (y * self.height / size).min(self.height - 1);
        self.rgb[(sy * self.width + sx) * 3 + c]
    }
}

pub struct Models {
    movenet: Option<Plan>,
    detector: Option<(Plan, usize)>,
}

impl Models {
    pub fn load(dir: &std::path::Path, need_pose: bool, need_detector: bool) -> Result<Self, String> {
        let movenet = if need_pose {
            Some(plan(&load_verified(dir, MOVENET)?, i32::fact([1, 192, 192, 3]).into())?)
        } else {
            None
        };
        let detector = if need_detector {
            let side = 300;
            Some((plan(&load_verified(dir, DETECTOR)?, u8::fact([1, side, side, 3]).into())?, side))
        } else {
            None
        };
        Ok(Self { movenet, detector })
    }

    /// MoveNet (17 COCO points) in the 33-point MediaPipe format that `RepCounter` expects.
    pub fn pose_as_mediapipe(&self, f: &Frame) -> Option<Vec<f32>> {
        let model = self.movenet.as_ref()?;
        const S: usize = 192;
        let input: Tensor =
            tract_ndarray::Array4::from_shape_fn((1, S, S, 3), |(_, y, x, c)| f.sample(S, x, y, c) as i32).into();
        let out = model.run(tvec!(input.into())).ok()?;
        let kp = out.first()?.to_plain_array_view::<f32>().ok()?;
        let flat: Vec<f32> = kp.iter().copied().collect(); // [1,1,17,3] = (y, x, score)
        movenet_to_mediapipe(&flat)
    }

    /// Is there a person and a book in the frame?
    pub fn person_and_book(&self, f: &Frame) -> (bool, bool) {
        let Some((model, side)) = self.detector.as_ref() else { return (false, false) };
        let side = *side;
        let input: Tensor =
            tract_ndarray::Array4::from_shape_fn((1, side, side, 3), |(_, y, x, c)| f.sample(side, x, y, c)).into();
        let Ok(out) = model.run(tvec!(input.into())) else { return (false, false) };
        // TF Object Detection export: [boxes, classes, scores, count] VERIFY: check in spike E
        let view = |i: usize| -> Option<Vec<f32>> {
            out.get(i).and_then(|t| t.to_plain_array_view::<f32>().ok()).map(|a| a.iter().copied().collect())
        };
        let (Some(classes), Some(scores)) = (view(1), view(2)) else { return (false, false) };
        person_and_book(&classes, &scores)
    }
}

/// MoveNet → MediaPipe mapping (indices used by `RepCounter`); the rest stays invisible.
pub fn movenet_to_mediapipe(kp: &[f32]) -> Option<Vec<f32>> {
    if kp.len() != 51 {
        return None;
    }
    const MAP: [(usize, usize); 17] = [
        (0, 0),
        (1, 2),
        (2, 5),
        (3, 7),
        (4, 8),
        (5, 11),
        (6, 12),
        (7, 13),
        (8, 14),
        (9, 15),
        (10, 16),
        (11, 23),
        (12, 24),
        (13, 25),
        (14, 26),
        (15, 27),
        (16, 28),
    ];
    let mut out = vec![0.0f32; 99];
    for (coco, mp) in MAP {
        let (y, x, score) = (kp[coco * 3], kp[coco * 3 + 1], kp[coco * 3 + 2]);
        out[mp * 3] = x;
        out[mp * 3 + 1] = y;
        out[mp * 3 + 2] = score;
    }
    Some(out)
}

pub fn person_and_book(classes: &[f32], scores: &[f32]) -> (bool, bool) {
    let seen = |id: i64| classes.iter().zip(scores).any(|(c, s)| *c as i64 == id && *s >= DETECT_MIN_SCORE);
    (seen(COCO_PERSON), seen(COCO_BOOK))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movenet_points_land_where_rep_counter_reads_them() {
        let mut kp = vec![0.0f32; 51];
        kp[13 * 3] = 0.6; // left knee: y
        kp[13 * 3 + 1] = 0.4; // x
        kp[13 * 3 + 2] = 0.9; // score
        let mp = movenet_to_mediapipe(&kp).unwrap();
        assert_eq!(&mp[25 * 3..25 * 3 + 3], &[0.4, 0.6, 0.9]);
        assert!(movenet_to_mediapipe(&[0.0; 10]).is_none());
    }

    #[test]
    fn detector_output_needs_confident_person_and_book() {
        assert_eq!(person_and_book(&[1.0, 84.0], &[0.9, 0.7]), (true, true));
        assert_eq!(person_and_book(&[1.0, 84.0], &[0.9, 0.2]), (true, false));
        assert_eq!(person_and_book(&[62.0], &[0.99]), (false, false));
    }

    #[test]
    fn unpinned_or_altered_models_are_refused() {
        let dir = std::env::temp_dir().join(format!("pco-models-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("m.onnx"), b"modelo").unwrap();
        assert!(load_verified(&dir, ("m.onnx", "PENDIENTE_SPIKE_E")).is_err());
        let good = hex::encode(Sha256::digest(b"modelo"));
        assert!(load_verified(&dir, ("m.onnx", &good)).is_ok());
        let wrong = hex::encode(Sha256::digest(b"otro"));
        assert!(load_verified(&dir, ("m.onnx", &wrong)).unwrap_err().contains("tampered"));
        std::fs::remove_dir_all(dir).ok();
    }
}
