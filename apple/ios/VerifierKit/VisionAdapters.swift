// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import CoreVideo
import Vision

/// Vision results in the formats the `verify` crate expects (the MediaPipe ones Android uses).
/// Vision's origin is bottom-left (y up); MediaPipe's is top-left: hence `y' = 1 - y`.
/// Everything stays in memory.
enum VisionAdapters {
    private static let bodyMap: [VNHumanBodyPoseObservation.JointName: Int] = [
        .nose: 0,
        .leftShoulder: 11, .rightShoulder: 12, .leftElbow: 13, .rightElbow: 14, .leftWrist: 15, .rightWrist: 16,
        .leftHip: 23, .rightHip: 24, .leftKnee: 25, .rightKnee: 26, .leftAnkle: 27, .rightAnkle: 28,
    ]

    private static let handOrder: [VNHumanHandPoseObservation.JointName] = [
        .wrist, .thumbCMC, .thumbMP, .thumbIP, .thumbTip, .indexMCP, .indexPIP, .indexDIP, .indexTip,
        .middleMCP, .middlePIP, .middleDIP, .middleTip, .ringMCP, .ringPIP, .ringDIP, .ringTip,
        .littleMCP, .littlePIP, .littleDIP, .littleTip,
    ]

    private static func perform(_ request: VNRequest, _ buffer: CVPixelBuffer) {
        try? VNImageRequestHandler(cvPixelBuffer: buffer).perform([request])
    }

    /// 33 × (x, y, visibility) for `RepCounter.feed`. Points Vision does not have stay at 0 (invisible).
    static func pose(_ buffer: CVPixelBuffer) -> [Float]? {
        let request = VNDetectHumanBodyPoseRequest()
        perform(request, buffer)
        guard let obs = request.results?.first, let points = try? obs.recognizedPoints(.all) else { return nil }
        var out = [Float](repeating: 0, count: 99)
        for (joint, i) in bodyMap {
            guard let p = points[joint] else { continue }
            out[i * 3] = Float(p.location.x)
            out[i * 3 + 1] = Float(1 - p.location.y)
            out[i * 3 + 2] = p.confidence
        }
        return out
    }

    /// 21 × (x, y, z) in MediaPipe Hands order, for `countFingers`.
    static func hand(_ buffer: CVPixelBuffer) -> [Float]? {
        let request = VNDetectHumanHandPoseRequest()
        request.maximumHandCount = 1
        perform(request, buffer)
        guard let obs = request.results?.first, let points = try? obs.recognizedPoints(.all) else { return nil }
        var out: [Float] = []
        for joint in handOrder {
            let p = points[joint]
            out += [Float(p?.location.x ?? 0), Float(1 - (p?.location.y ?? 1)), 0]
        }
        return out
    }

    /// The system classifier's labels (food, and whether there is a book).
    /// VERIFY: check the real identifiers with `VNClassifyImageRequest().supportedIdentifiers()` and
    /// adjust verify/src/food.rs and `hasBook`.
    static func labels(_ buffer: CVPixelBuffer) -> [Label] {
        let request = VNClassifyImageRequest()
        perform(request, buffer)
        return (request.results ?? []).filter { $0.confidence > 0.1 }.prefix(10)
            .map { Label(name: $0.identifier, score: UInt8(min(100, Int($0.confidence * 100)))) }
    }

    static func hasFace(_ buffer: CVPixelBuffer) -> Bool {
        let request = VNDetectFaceRectanglesRequest()
        perform(request, buffer)
        return !(request.results ?? []).isEmpty
    }

    static func hasBook(_ buffer: CVPixelBuffer) -> Bool {
        labels(buffer).contains { $0.name == "book" && $0.score >= 40 }
    }

    /// Page text (memory only) and its lines with their vertical position in pixels from the top,
    /// for `pageNumber(lines:imageHeight:)`.
    static func readPage(_ buffer: CVPixelBuffer) -> (text: String, lines: [OcrLine], height: Int32) {
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = false // page numbers must stay as they are
        perform(request, buffer)
        let height = Double(CVPixelBufferGetHeight(buffer))
        let lines: [OcrLine] = (request.results ?? []).compactMap { obs in
            guard let text = obs.topCandidates(1).first?.string else { return nil }
            let box = obs.boundingBox
            return OcrLine(text: text, top: Int32((1 - box.maxY) * height), bottom: Int32((1 - box.minY) * height))
        }
        return (lines.map(\.text).joined(separator: "\n"), lines, Int32(height))
    }

    /// 9×8 gray bytes for `dhash` (each cell is the average of its area). BGRA buffers only.
    static func gray9x8(_ buffer: CVPixelBuffer) -> Data? {
        guard CVPixelBufferGetPixelFormatType(buffer) == kCVPixelFormatType_32BGRA else { return nil }
        CVPixelBufferLockBaseAddress(buffer, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(buffer, .readOnly) }
        guard let base = CVPixelBufferGetBaseAddress(buffer) else { return nil }
        let (w, h, row) = (CVPixelBufferGetWidth(buffer), CVPixelBufferGetHeight(buffer), CVPixelBufferGetBytesPerRow(buffer))
        guard w >= 9, h >= 8 else { return nil }
        let px = base.assumingMemoryBound(to: UInt8.self)
        var out = Data(count: 72)
        for cy in 0..<8 {
            for cx in 0..<9 {
                var (sum, n) = (0, 0)
                // ponytail: every 2nd pixel of the cell; enough for a 64-bit hash of a 640×480 frame
                for y in stride(from: cy * h / 8, to: (cy + 1) * h / 8, by: 2) {
                    for x in stride(from: cx * w / 9, to: (cx + 1) * w / 9, by: 2) {
                        let p = px + y * row + x * 4 // B, G, R, A
                        sum += (Int(p[2]) * 30 + Int(p[1]) * 59 + Int(p[0]) * 11) / 100
                        n += 1
                    }
                }
                out[cy * 9 + cx] = UInt8(sum / max(n, 1))
            }
        }
        return out
    }
}
