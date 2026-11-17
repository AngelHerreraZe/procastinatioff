// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import AVFoundation

enum VerifierError: Error {
    case noCamera
}

/// Camera frames in memory only (`AVCaptureVideoDataOutput`, low resolution), at most one every
/// `every` seconds, upright for a phone held in portrait. SECURITY: nothing is written anywhere;
/// AVFoundation releases the buffer when `onFrame` returns (`privacy_scan.py` forbids the outputs
/// that save photos or video, and any network or disk API in VerifierKit).
final class FrameSource: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate {
    let session = AVCaptureSession()
    private let queue = DispatchQueue(label: "dev.procastinatioff.frames")
    private let everyMs: Int64
    private let onFrame: (CVPixelBuffer, Int64) -> Void
    private var lastMs: Int64?

    /// `onFrame(buffer, atMs)`: `atMs` is monotonic (presentation time), for `ReadingTracker` and `RepCounter`.
    init(every: TimeInterval, front: Bool, onFrame: @escaping (CVPixelBuffer, Int64) -> Void) throws {
        everyMs = Int64(every * 1000)
        self.onFrame = onFrame
        super.init()
        session.sessionPreset = .vga640x480
        guard let device = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: front ? .front : .back)
        else { throw VerifierError.noCamera }
        let input = try AVCaptureDeviceInput(device: device)
        let output = AVCaptureVideoDataOutput()
        output.alwaysDiscardsLateVideoFrames = true
        output.videoSettings = [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA]
        output.setSampleBufferDelegate(self, queue: queue)
        guard session.canAddInput(input), session.canAddOutput(output) else { throw VerifierError.noCamera }
        session.addInput(input)
        session.addOutput(output)
        // Vision reads the buffers as they come: rotate them to portrait once here.
        if let connection = output.connection(with: .video) {
            if #available(iOS 17.0, *) {
                if connection.isVideoRotationAngleSupported(90) { connection.videoRotationAngle = 90 }
            } else if connection.isVideoOrientationSupported {
                connection.videoOrientation = .portrait
            }
        }
    }

    /// `startRunning` blocks: never on the main thread.
    func start() { queue.async { self.session.startRunning() } }

    func stop() { queue.async { self.session.stopRunning() } }

    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
        let atMs = Int64(CMTimeGetSeconds(CMSampleBufferGetPresentationTimeStamp(sampleBuffer)) * 1000)
        if let lastMs, atMs - lastMs < everyMs { return }
        guard let buffer = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }
        lastMs = atMs
        onFrame(buffer, atMs)
    }
}
