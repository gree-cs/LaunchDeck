import AppKit
import Foundation

// 用法: icongen <outdir> <app path>...
// 输出: icon_<n>.png + manifest.json { appPath: fileName }
let args = CommandLine.arguments
guard args.count >= 3 else {
    FileHandle.standardError.write("usage: icongen <outdir> <paths...>\n".data(using: .utf8)!)
    exit(2)
}
let outDir = args[1]
try? FileManager.default.createDirectory(atPath: outDir, withIntermediateDirectories: true)

let size: CGFloat = 256

func fnv1a(_ s: String) -> UInt64 {
    var h: UInt64 = 0xcbf29ce484222325
    for b in s.utf8 {
        h ^= UInt64(b)
        h = h &* 0x100000001b3
    }
    return h
}

var manifest: [String: String] = [:]
for path in args.dropFirst(2) {
    let icon = NSWorkspace.shared.icon(forFile: path)

    // 显式绘制到 128x128 画布，避免导出 1024 原图
    let canvas = NSImage(size: NSSize(width: size, height: size))
    canvas.lockFocus()
    NSGraphicsContext.current?.imageInterpolation = .high
    icon.draw(in: NSRect(x: 0, y: 0, width: size, height: size),
              from: .zero,
              operation: .sourceOver,
              fraction: 1.0)
    canvas.unlockFocus()

    guard let tiff = canvas.tiffRepresentation,
          let rep = NSBitmapImageRep(data: tiff),
          let png = rep.representation(using: .png, properties: [:]) else { continue }

    let name = String(format: "ic_%016llx.png", fnv1a(path))
    let dst = (outDir as NSString).appendingPathComponent(name)
    if (try? png.write(to: URL(fileURLWithPath: dst))) != nil {
        manifest[path] = name
    }
}
let data = try JSONSerialization.data(withJSONObject: manifest, options: [])
try data.write(to: URL(fileURLWithPath: (outDir as NSString).appendingPathComponent("manifest.json")))
