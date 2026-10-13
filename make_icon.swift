// Original app icon for ralfred (clean-room: no Alfred assets).
// Concept: a rounded search panel bar over a soft gradient tile.
import AppKit

func drawIcon(size s: CGFloat) -> Data? {
    let image = NSImage(size: NSSize(width: s, height: s))
    image.lockFocus()
    let rect = NSRect(x: 0, y: 0, width: s, height: s)
    let inset = s * 0.08
    let tile = NSRect(x: inset, y: inset, width: s - 2 * inset, height: s - 2 * inset)
    let tilePath = NSBezierPath(roundedRect: tile, xRadius: s * 0.22, yRadius: s * 0.22)
    let grad = NSGradient(colors: [
        NSColor(red: 0.18, green: 0.24, blue: 0.42, alpha: 1),
        NSColor(red: 0.06, green: 0.08, blue: 0.16, alpha: 1),
    ])!
    grad.draw(in: tilePath, angle: -90)
    // panel bar
    let pw = tile.width * 0.62
    let ph = tile.height * 0.16
    let bar = NSRect(x: tile.midX - pw/2, y: tile.midY - ph/2, width: pw, height: ph)
    let barPath = NSBezierPath(roundedRect: bar, xRadius: ph/2, yRadius: ph/2)
    NSColor.white.withAlphaComponent(0.94).setFill()
    barPath.fill()
    // magnifier dot + handle in bar (indigo)
    NSColor(red: 0.25, green: 0.31, blue: 0.55, alpha: 1).setFill()
    let r = ph * 0.34
    let dot = NSRect(x: bar.minX + ph*0.38, y: bar.midY - r/2, width: r, height: r)
    NSBezierPath(ovalIn: dot).fill()
    let handle = NSBezierPath()
    handle.move(to: NSPoint(x: dot.maxX + r*0.15, y: bar.midY - r*0.45))
    handle.line(to: NSPoint(x: dot.maxX + r*0.55, y: bar.midY + r*0.6))
    handle.lineWidth = ph * 0.14
    handle.lineCapStyle = .round
    handle.stroke()
    image.unlockFocus()
    guard let tiff = image.tiffRepresentation,
          let bmp = NSBitmapImageRep(data: tiff) else { return nil }
    return bmp.representation(using: .png, properties: [:])
}

let set = "AppIcon.iconset"
try? FileManager.default.createDirectory(atPath: set, withIntermediateDirectories: true)
let sizes: [(String, Int)] = [
    ("16x16",16),("16x16@2x",32),("32x32",32),("32x32@2x",64),
    ("128x128",128),("128x128@2x",256),("256x256",256),("256x256@2x",512),("512x512",512),("512x512@2x",1024)
]
for (name, px) in sizes {
    if let png = drawIcon(size: CGFloat(px)) {
        try! png.write(to: URL(fileURLWithPath: "\(set)/icon_\(name).png"))
    }
}
print("iconset written")
