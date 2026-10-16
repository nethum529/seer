import AppKit
import CoreGraphics

let output = CommandLine.arguments[2]
if CommandLine.arguments[1] == "display" {
    exit(CGSessionCopyCurrentDictionary() != nil && CGDisplayPixelsWide(CGMainDisplayID()) > 0 ? 0 : 77)
}
if CommandLine.arguments[1] != "measure" {
    let pid = Int32(CommandLine.arguments[1])!
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
    let window = windows.first { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }!
    let id = CGWindowID(window[kCGWindowNumber as String] as! Int)
    typealias Capture = @convention(c) (CGRect, UInt32, UInt32, UInt32) -> Unmanaged<CGImage>?
    let library = dlopen("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics", RTLD_LAZY)!
    let capture = unsafeBitCast(dlsym(library, "CGWindowListCreateImage")!, to: Capture.self)
    let image = capture(.null, 1 << 3, id, (1 << 0) | (1 << 3))!.takeRetainedValue()
    let data = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])!
    try! data.write(to: URL(fileURLWithPath: output + "/" + CommandLine.arguments[3] + ".png"))
    exit(0)
}
var table = "Label | Scale | Top | Bottom | Difference\n"
var failures = 0
let samples: [(String, String, [Double])] = [
    ("main", "Sidebar name", [44, 44, 10, 36]),
    ("main", "Sidebar avatar", [22, 44, 9, 36]),
    ("main", "Sidebar tab", [44, 92, 8, 33]),
    ("main", "New terminal", [44, 131, 9, 32]),
    ("main", "Top bar title", [590, 7.5, 8, 28]),
    ("people", "Person name", [53, 89, 10, 36]),
    ("people", "Person avatar", [28, 96, 8, 22]),
    ("people", "Badge", [89, 89, 8, 36]),
    ("settings", "Settings heading", [145, 145, 9, 32]),
    ("settings", "Settings navigation", [145, 193, 9, 32]),
    ("settings", "Settings title", [321, 145, 10, 32]),
    ("settings", "Settings row", [321, 201, 9, 18]),
    ("settings", "Settings switch", [745, 205, 9, 32]),
    ("settings", "Button", [335, 421, 9, 32]),
    ("main", "Room trigger", [44, 602.5, 8, 32]),
    ("room", "Room name", [53, 476.5, 8, 36]),
    ("room", "Room action", [53, 521.5, 8, 36]),
    ("sheet", "Sheet title", [301, 262, 9, 20]),
    ("sheet", "Sheet button", [500, 346, 9, 32]),
    ("context", "Menu item", [99, 113, 9, 32]),
    ("tooltip", "Tooltip", [18, 84, 9, 27]),
]
for (capture, name, box) in samples {
    let image = NSBitmapImageRep(data: try! Data(contentsOf: URL(fileURLWithPath: output + "/" + capture + ".png")))!
    let scale = Double(image.pixelsWide) / 960
    precondition(scale == 1 || scale == 2, "expected a full native window capture at 1x or 2x")
    let values = box.map { Int(($0 * scale).rounded()) }
    let (x, y, width, height) = (values[0], values[1], values[2], values[3])
    let background = image.colorAt(x: x - Int(2 * scale), y: y + height / 2)!.usingColorSpace(.deviceRGB)!
    let rows = (Int(scale)..<height-Int(scale)).filter { row in
        (0..<width).contains { col in
            let color = image.colorAt(x: x + col, y: y + row)!.usingColorSpace(.deviceRGB)!
            let contrast = background.redComponent > 0.5 ? background.redComponent - color.redComponent : color.redComponent - background.redComponent
            let channels = [color.redComponent, color.greenComponent, color.blueComponent]
            return contrast > 8.0 / 255 && channels.max()! - channels.min()! < 4.0 / 255
        }
    }
    precondition(!rows.isEmpty, "no text in " + name)
    let top = rows.first!
    let bottom = height - rows.last! - 1
    let difference = abs(top - bottom)
    table += "\(name) | \(scale) | \(top) | \(bottom) | \(difference)\n"
    if difference > 1 { failures += 1 }
    let crop = image.cgImage!.cropping(to: CGRect(x: x, y: y, width: width, height: height))!
    try! NSBitmapImageRep(cgImage: crop).representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: output + "/crop-" + name.replacingOccurrences(of: " ", with: "-") + ".png"))
}
try! table.write(toFile: output + "/measurements.txt", atomically: true, encoding: .utf8)
print(table)
exit(failures == 0 ? 0 : 1)
