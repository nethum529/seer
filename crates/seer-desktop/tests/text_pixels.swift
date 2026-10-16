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
var table = "Label | Scale | Above capitals | Below baseline | Difference\n"
var failures = 0
let sidebarStates = ["shown-lines", "hidden-lines", "shown-no-lines", "hidden-no-lines"]
var samples: [(String, String, [Double], [Double])] = [
    ("main", "Sidebar name", [44, 52, 10, 36], [44, 10]),
    ("main", "Sidebar avatar", [22, 59, 9, 22], [22, 9]),
    ("main", "Sidebar tab", [44, 92, 8, 33], [68, 6]),
    ("main", "New terminal", [44, 131, 9, 32], [44, 9]),
    ("main", "Top bar title", [590, 7.5, 8, 28], [613, 6]),
    ("people", "Person name", [53, 97, 10, 36], [53, 10]),
    ("people", "Person avatar", [28, 104, 8, 22], [28, 8]),
    ("people", "Badge", [89, 97, 8, 36], [89, 8]),
    ("settings", "Settings heading", [145, 145, 9, 32], [175, 2]),
    ("settings", "Settings navigation", [145, 193, 9, 32], [145, 9]),
    ("settings", "Settings title", [321, 145, 10, 32], [321, 10]),
    ("settings", "Settings row", [321, 208, 9, 18], [321, 4]),
    ("settings", "Settings switch", [745, 201, 9, 32], [756, 2]),
    ("settings", "Button", [335, 419, 9, 32], [395, 2]),
    ("main", "Room trigger", [44, 602.5, 8, 32], [44, 8]),
    ("room", "Room name", [53, 476.5, 8, 36], [53, 8]),
    ("room", "Room action", [53, 521.5, 8, 36], [53, 8]),
    ("sheet", "Sheet title", [301, 262, 9, 20], [339, 3]),
    ("sheet", "Sheet button", [500, 346, 9, 32], [541, 2]),
    ("context", "Menu item", [99, 113, 9, 32], [99, 9]),
    ("tooltip", "Tooltip", [18, 92, 9, 27], [18, 9]),
]
for state in sidebarStates {
    let bar = state == "hidden-no-lines" ? 0.0 : 44.0
    let capture = "sidebar-" + state
    samples += [
        (capture, state + " name", [44, bar + 8, 10, 36], [44, 10]),
        (capture, state + " avatar", [22, bar + 15, 9, 22], [22, 9]),
        (capture, state + " tab", [44, bar + 48, 8, 33], [68, 6]),
        (capture, state + " New terminal", [44, bar + 87, 9, 32], [44, 9]),
    ]
}
for (capture, name, box, probe) in samples {
    let image = NSBitmapImageRep(data: try! Data(contentsOf: URL(fileURLWithPath: output + "/" + capture + ".png")))!
    let scale = Double(image.pixelsWide) / 960
    precondition(scale == 1 || scale == 2, "expected a full native window capture at 1x or 2x")
    let values = box.map { Int(($0 * scale).rounded()) }
    let (x, y, width, height) = (values[0], values[1], values[2], values[3])
    let background = image.colorAt(x: x - Int(2 * scale), y: y + height / 2)!.usingColorSpace(.deviceRGB)!
    let inkRows = { (left: Int, width: Int) in
        (Int(scale)..<height-Int(scale)).filter { row in
            (0..<width).contains { col in
                let color = image.colorAt(x: left + col, y: y + row)!.usingColorSpace(.deviceRGB)!
                let channels = [color.redComponent, color.greenComponent, color.blueComponent]
                let mean = channels.reduce(0, +) / 3
                let reference = (background.redComponent + background.greenComponent + background.blueComponent) / 3
                let contrast = reference > 0.5 ? reference - mean : mean - reference
                return contrast > 8.0 / 255 && channels.max()! - channels.min()! < 4.0 / 255
            }
        }
    }
    let capitals = inkRows(x, width)
    let stemX = Int((probe[0] * scale).rounded())
    let stemWidth = Int((probe[1] * scale).rounded())
    let stem = inkRows(stemX, stemWidth)
    precondition(!capitals.isEmpty && !stem.isEmpty, "no text in " + name)
    let top = capitals.first!
    let bottom = height - stem.last! - 1
    let difference = abs(top - bottom)
    table += "\(name) | \(scale) | \(top) | \(bottom) | \(difference)\n"
    if difference > 1 { failures += 1 }
    let left = min(x, stemX)
    let right = max(x + width, stemX + stemWidth)
    let crop = image.cgImage!.cropping(to: CGRect(x: left, y: y, width: right - left, height: height))!
    try! NSBitmapImageRep(cgImage: crop).representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: output + "/crop-" + name.replacingOccurrences(of: " ", with: "-") + ".png"))
}
for state in sidebarStates {
    let image = NSBitmapImageRep(data: try! Data(contentsOf: URL(fileURLWithPath: output + "/sidebar-" + state + ".png")))!
    let scale = Double(image.pixelsWide) / 960
    let bar = state == "hidden-no-lines" ? 0.0 : 44.0
    let avatarFill = image.colorAt(x: Int(17 * scale), y: Int((bar + 22) * scale))!.usingColorSpace(.deviceRGB)!.redComponent
    let avatar = (Int(bar * scale)..<Int((bar + 48) * scale)).filter { y in
        let color = image.colorAt(x: Int(17 * scale), y: y)!.usingColorSpace(.deviceRGB)!
        return abs(color.redComponent - avatarFill) < 0.002
    }
    precondition(!avatar.isEmpty, "avatar is missing in " + state)
    let inset = Double(avatar.first! + avatar.last! + 1) / 2 - (bar + 18) * scale
    let rowInset = 8 * scale
    let selectedFill = image.colorAt(x: Int(200 * scale), y: Int((bar + 60) * scale))!.usingColorSpace(.deviceRGB)!.redComponent
    let tabTop = (Int((bar + 36) * scale)..<Int((bar + 80) * scale)).first { y in
        let color = image.colorAt(x: Int(100 * scale), y: y)!.usingColorSpace(.deviceRGB)!
        return abs(color.redComponent - selectedFill) < 0.002
    }
    let aligned = tabTop == Int((bar + 48) * scale)
    table += "\(state) | top inset \(inset) | side inset \(rowInset) | tab aligned \(aligned)\n"
    if abs(inset - rowInset) > 0.5 || !aligned { failures += 1 }
}
try! table.write(toFile: output + "/measurements.txt", atomically: true, encoding: .utf8)
print(table + "Failed checks: \(failures) / \(samples.count + 4)")
exit(failures == 0 ? 0 : 1)
