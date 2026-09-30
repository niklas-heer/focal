// Renders Focal's app icon: an "F" in iA Writer Duo with a blue caret on a
// paper-colored rounded square, at every size macOS asks for.
//   swift packaging/make-icon.swift <fonts dir> <output .iconset dir>
import AppKit
import CoreText

let args = CommandLine.arguments
guard args.count == 3 else {
    FileHandle.standardError.write("usage: make-icon.swift <fonts dir> <iconset dir>\n".data(using: .utf8)!)
    exit(64)
}
let fontURL = URL(fileURLWithPath: args[1]).appendingPathComponent("iAWriterDuoS-Bold.ttf")
CTFontManagerRegisterFontsForURL(fontURL as CFURL, .process, nil)
let output = URL(fileURLWithPath: args[2])
try? FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)

func render(_ pixels: Int) -> Data {
    let size = CGFloat(pixels)
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
        bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let unit = size / 1024
    // Apple's grid: an 824-point body with room for the shadow.
    let body = NSRect(x: 100 * unit, y: 100 * unit, width: 824 * unit, height: 824 * unit)
    let path = NSBezierPath(roundedRect: body, xRadius: 185 * unit, yRadius: 185 * unit)
    let shadow = NSShadow()
    shadow.shadowColor = NSColor(white: 0, alpha: 0.28)
    shadow.shadowOffset = NSSize(width: 0, height: -10 * unit)
    shadow.shadowBlurRadius = 24 * unit
    NSGraphicsContext.saveGraphicsState()
    shadow.set()
    NSColor(srgbRed: 0.969, green: 0.957, blue: 0.933, alpha: 1).setFill()
    path.fill()
    NSGraphicsContext.restoreGraphicsState()
    NSColor(white: 0, alpha: 0.08).setStroke()
    path.lineWidth = 2 * unit
    path.stroke()

    let font = NSFont(name: "iAWriterDuoS-Bold", size: 560 * unit)
        ?? NSFont.boldSystemFont(ofSize: 560 * unit)
    let letter = NSAttributedString(string: "F", attributes: [
        .font: font,
        .foregroundColor: NSColor(srgbRed: 0.13, green: 0.13, blue: 0.14, alpha: 1),
    ])
    // Center the capital between baseline and cap height, letter and caret together.
    let width = letter.size().width
    let caretWidth = 34 * unit
    let gap = 30 * unit
    let left = (size - width - gap - caretWidth) / 2
    let baseline = (size - font.capHeight) / 2
    letter.draw(at: NSPoint(x: left, y: baseline + font.descender))
    NSColor(srgbRed: 0.24, green: 0.52, blue: 0.95, alpha: 1).setFill()
    let caret = NSRect(
        x: left + width + gap, y: baseline - 40 * unit,
        width: caretWidth, height: font.capHeight + 80 * unit)
    NSBezierPath(roundedRect: caret, xRadius: caretWidth / 2, yRadius: caretWidth / 2).fill()
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

for points in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let name = scale == 1 ? "icon_\(points)x\(points).png" : "icon_\(points)x\(points)@2x.png"
        try render(points * scale).write(to: output.appendingPathComponent(name))
    }
}
