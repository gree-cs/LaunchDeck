import AppKit
import Foundation

// 把 SF Symbols 渲染成 PNG（2×），供 egui 侧当图标使用。
// 用法: symbolgen <输出目录>

let outDir = CommandLine.arguments[1]
try? FileManager.default.createDirectory(atPath: outDir, withIntermediateDirectories: true)

struct Spec {
    let name: String
    let pointSize: CGFloat
    let weight: NSFont.Weight
}

let specs: [Spec] = [
    Spec(name: "gearshape", pointSize: 17, weight: .regular),
    Spec(name: "slider.horizontal.3", pointSize: 17, weight: .regular),
    Spec(name: "square.grid.2x2", pointSize: 17, weight: .regular),
    Spec(name: "switch.2", pointSize: 17, weight: .regular),
    Spec(name: "info.circle", pointSize: 17, weight: .regular),
    Spec(name: "magnifyingglass", pointSize: 15, weight: .regular),
    Spec(name: "clock", pointSize: 13, weight: .regular),
    Spec(name: "xmark", pointSize: 10, weight: .semibold),
]

let scale: CGFloat = 2.0
var manifest: [String: String] = [:]

for spec in specs {
    guard let base = NSImage(systemSymbolName: spec.name, accessibilityDescription: nil) else {
        FileHandle.standardError.write("missing symbol \(spec.name)\n".data(using: .utf8)!)
        continue
    }
    let cfg = NSImage.SymbolConfiguration(pointSize: spec.pointSize, weight: spec.weight)
    guard let sym = base.withSymbolConfiguration(cfg) else { continue }
    let w = Int(ceil(sym.size.width * scale))
    let h = Int(ceil(sym.size.height * scale))
    guard w > 0, h > 0,
          let rep = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: w, pixelsHigh: h,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)
    else { continue }

    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    NSColor.white.set()
    sym.draw(in: NSRect(x: 0, y: 0, width: w, height: h))
    NSGraphicsContext.restoreGraphicsState()

    let file = "sym_" + spec.name.replacingOccurrences(of: ".", with: "_") + ".png"
    if let data = rep.representation(using: .png, properties: [:]) {
        try? data.write(to: URL(fileURLWithPath: outDir + "/" + file))
        manifest[spec.name] = file
    }
}

if let data = try? JSONSerialization.data(withJSONObject: manifest, options: [.prettyPrinted]) {
    try? data.write(to: URL(fileURLWithPath: outDir + "/symbols.json"))
}
print("symbols: \(manifest.count)")
