// Writes the media fixtures MEDIA-AVF tests and pins use (docs/design/MEDIA_AVF.md §5),
// with AVAssetWriter: the OS's own encoders, no third-party encoder.
//
//   color-steps.mp4   64x48, 2 s at 10 fps, H.264 + AAC. Frame n is a solid colour from
//                     COLOURS (index n % 10), so one pixel names the frame; the audio is a
//                     440 Hz sine at -12 dBFS, 44.1 kHz mono.
//   size-32x24.mp4    32x24, one frame (red), H.264, no audio: the 4:3 size pin.
//
// usage: swift tools/media_fixtures/make_fixtures.swift fixtures/media
// The files are committed; rerun only to change them (the bytes differ run to run, the
// decoded frames and sizes do not).
import AVFoundation
import CoreMedia
import CoreVideo
import Foundation

let COLOURS: [(UInt8, UInt8, UInt8)] = [
    (255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 0), (0, 255, 255),
    (255, 0, 255), (255, 255, 255), (0, 0, 0), (255, 128, 0), (128, 0, 255),
]

func pixelBuffer(_ w: Int, _ h: Int, _ c: (UInt8, UInt8, UInt8)) -> CVPixelBuffer {
    var pb: CVPixelBuffer?
    let attrs = [kCVPixelBufferCGImageCompatibilityKey: true, kCVPixelBufferCGBitmapContextCompatibilityKey: true] as CFDictionary
    CVPixelBufferCreate(kCFAllocatorDefault, w, h, kCVPixelFormatType_32BGRA, attrs, &pb)
    let buf = pb!
    CVPixelBufferLockBaseAddress(buf, [])
    let base = CVPixelBufferGetBaseAddress(buf)!.assumingMemoryBound(to: UInt8.self)
    let stride = CVPixelBufferGetBytesPerRow(buf)
    for y in 0..<h {
        for x in 0..<w {
            let p = base + y * stride + x * 4
            p[0] = c.2; p[1] = c.1; p[2] = c.0; p[3] = 255
        }
    }
    CVPixelBufferUnlockBaseAddress(buf, [])
    return buf
}

func audioSample(start: Int, count: Int, rate: Double) -> CMSampleBuffer {
    var asbd = AudioStreamBasicDescription(
        mSampleRate: rate, mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kLinearPCMFormatFlagIsSignedInteger | kLinearPCMFormatFlagIsPacked,
        mBytesPerPacket: 2, mFramesPerPacket: 1, mBytesPerFrame: 2, mChannelsPerFrame: 1,
        mBitsPerChannel: 16, mReserved: 0)
    var format: CMAudioFormatDescription?
    CMAudioFormatDescriptionCreate(allocator: kCFAllocatorDefault, asbd: &asbd, layoutSize: 0, layout: nil,
                                   magicCookieSize: 0, magicCookie: nil, extensions: nil, formatDescriptionOut: &format)
    var samples = [Int16](repeating: 0, count: count)
    for i in 0..<count {
        let t = Double(start + i) / rate
        samples[i] = Int16(sin(2 * Double.pi * 440 * t) * 0.25 * 32767)
    }
    var block: CMBlockBuffer?
    let bytes = count * 2
    CMBlockBufferCreateWithMemoryBlock(allocator: kCFAllocatorDefault, memoryBlock: nil, blockLength: bytes,
                                       blockAllocator: kCFAllocatorDefault, customBlockSource: nil, offsetToData: 0,
                                       dataLength: bytes, flags: 0, blockBufferOut: &block)
    samples.withUnsafeBytes { raw in
        _ = CMBlockBufferReplaceDataBytes(with: raw.baseAddress!, blockBuffer: block!, offsetIntoDestination: 0, dataLength: bytes)
    }
    var sample: CMSampleBuffer?
    CMAudioSampleBufferCreateReadyWithPacketDescriptions(
        allocator: kCFAllocatorDefault, dataBuffer: block!, formatDescription: format!, sampleCount: count,
        presentationTimeStamp: CMTime(value: CMTimeValue(start), timescale: CMTimeScale(rate)),
        packetDescriptions: nil, sampleBufferOut: &sample)
    return sample!
}

func write(_ url: URL, w: Int, h: Int, frames: Int, fps: Int32, audioSeconds: Double?, colour: (Int) -> (UInt8, UInt8, UInt8)) {
    try? FileManager.default.removeItem(at: url)
    let writer = try! AVAssetWriter(outputURL: url, fileType: .mp4)
    let video = AVAssetWriterInput(mediaType: .video, outputSettings: [
        AVVideoCodecKey: AVVideoCodecType.h264, AVVideoWidthKey: w, AVVideoHeightKey: h,
        AVVideoCompressionPropertiesKey: [AVVideoProfileLevelKey: AVVideoProfileLevelH264BaselineAutoLevel,
                                          AVVideoMaxKeyFrameIntervalKey: 1],
    ])
    video.expectsMediaDataInRealTime = false
    let adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: video, sourcePixelBufferAttributes: nil)
    writer.add(video)
    var audio: AVAssetWriterInput?
    if audioSeconds != nil {
        let a = AVAssetWriterInput(mediaType: .audio, outputSettings: [
            AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: 44100, AVNumberOfChannelsKey: 1, AVEncoderBitRateKey: 64000,
        ])
        a.expectsMediaDataInRealTime = false
        writer.add(a)
        audio = a
    }
    writer.startWriting()
    writer.startSession(atSourceTime: .zero)
    // Feed both inputs as each is ready: the writer interleaves them and stops taking video
    // while it waits for audio of the same time, so one input at a time would deadlock.
    var n = 0
    let rate = 44100.0, chunk = 4410
    let total = Int((audioSeconds ?? 0) * rate)
    var at = 0
    var videoDone = false, audioDone = audio == nil
    while !videoDone || !audioDone {
        var progressed = false
        if !videoDone && video.isReadyForMoreMediaData {
            adaptor.append(pixelBuffer(w, h, colour(n)), withPresentationTime: CMTime(value: CMTimeValue(n), timescale: fps))
            n += 1
            progressed = true
            if n == frames {
                video.markAsFinished()
                videoDone = true
            }
        }
        if let a = audio, !audioDone && a.isReadyForMoreMediaData {
            a.append(audioSample(start: at, count: min(chunk, total - at), rate: rate))
            at += chunk
            progressed = true
            if at >= total {
                a.markAsFinished()
                audioDone = true
            }
        }
        if !progressed { usleep(1000) }
    }
    // The last frame lasts one frame interval.
    writer.endSession(atSourceTime: CMTime(value: CMTimeValue(frames), timescale: fps))
    let done = DispatchSemaphore(value: 0)
    writer.finishWriting { done.signal() }
    done.wait()
    if writer.status != .completed {
        fatalError("\(url.lastPathComponent): \(String(describing: writer.error))")
    }
    print("wrote \(url.path)")
}

let out = URL(fileURLWithPath: CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "fixtures/media")
try! FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)
write(out.appendingPathComponent("color-steps.mp4"), w: 64, h: 48, frames: 20, fps: 10, audioSeconds: 2.0) { COLOURS[$0 % COLOURS.count] }
write(out.appendingPathComponent("size-32x24.mp4"), w: 32, h: 24, frames: 1, fps: 10, audioSeconds: nil) { _ in (255, 0, 0) }
