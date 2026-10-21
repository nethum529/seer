import AppKit
import CoreGraphics

let output = CommandLine.arguments[2]
if CommandLine.arguments[1] == "display" {
    exit(CGSessionCopyCurrentDictionary() != nil && CGDisplayPixelsWide(CGMainDisplayID()) > 0 ? 0 : 77)
}
let appearanceOnly = CommandLine.arguments[1] == "measure-appearance"
if CommandLine.arguments[1] != "measure" && !appearanceOnly {
    let pid = Int32(CommandLine.arguments[1])!
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
    let processPanel = ProcessInfo.processInfo.environment["PROCESSES_QA"] != nil
    let window = windows.first { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && (($0[kCGWindowLayer as String] as? Int) == 0 || processPanel) }!
    let id = CGWindowID(window[kCGWindowNumber as String] as! Int)
    typealias Capture = @convention(c) (CGRect, UInt32, UInt32, UInt32) -> Unmanaged<CGImage>?
    let library = dlopen("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics", RTLD_LAZY)!
    let capture = unsafeBitCast(dlsym(library, "CGWindowListCreateImage")!, to: Capture.self)
    var image = capture(.null, 1 << 3, id, (1 << 0) | (1 << 3))!.takeRetainedValue()
    if ProcessInfo.processInfo.environment["COLOR_QA"] != nil {
        let context = CGContext(data: nil, width: image.width, height: image.height, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        image = context.makeImage()!
    }
    let data = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])!
    try! data.write(to: URL(fileURLWithPath: output + "/" + CommandLine.arguments[3] + ".png"))
    exit(0)
}
var table = "Label | Scale | Above capitals | Below baseline | Difference\n"
var failures = 0
let startRoomOnly = ProcessInfo.processInfo.environment["TEXT_PIXEL_START_ROOM_ONLY"] != nil || ProcessInfo.processInfo.environment["TEXT_PIXEL_START_SCREEN_ONLY"] != nil
let sidebarStates = startRoomOnly ? [] : ["shown-lines", "hidden-lines", "shown-no-lines", "hidden-no-lines"]
var samples: [(String, String, [Double], [Double])] = startRoomOnly ? [] : [
    ("main", "Sidebar name", [44, 52, 10, 36], [44, 10]),
    ("main", "Sidebar avatar", [22, 59, 9, 22], [22, 9]),
    ("main", "Sidebar tab", [44, 92, 8, 33], [68, 6]),
    ("main", "New terminal", [44, 131, 9, 32], [44, 9]),
    ("main", "Top bar title", [466, 7.5, 8, 28], [489, 6]),
    ("people", "Person name", [53, 97, 10, 36], [53, 10]),
    ("people", "Person avatar", [28, 104, 8, 22], [28, 8]),
    ("people", "Badge", [89, 97, 8, 36], [89, 8]),
    ("settings", "Settings heading", [145, 65, 9, 32], [175, 2]),
    ("settings", "Settings navigation", [145, 113, 9, 32], [145, 9]),
    ("settings", "Settings title", [321, 65, 10, 32], [321, 10]),
    ("settings", "Settings row", [321, 128, 9, 18], [321, 4]),
    ("main", "Room trigger", [44, 602.5, 8, 32], [44, 8]),
    ("room", "Room name", [53, 440.5, 8, 36], [53, 8]),
    ("room", "Room action", [53, 485.5, 8, 36], [53, 8]),
    ("sheet", "Sheet title", [301, 262, 9, 20], [339, 3]),
    ("sheet", "Sheet button", [500, 346, 9, 32], [541, 2]),
    ("context", "Menu item", [99, 113, 9, 32], [99, 9]),
    ("tooltip", "Tooltip", [18, 92, 9, 27], [18, 9]),
    ("terminal-tip", "Terminal shortcut tooltip", [18, 129, 9, 59], [18, 9]),
]
for capture in (startRoomOnly ? [] : ["toggle-show", "toggle-show-repeat"]) {
    samples += [
        (capture, capture + " name", [44, 52, 10, 36], [44, 10]),
        (capture, capture + " avatar", [22, 59, 9, 22], [22, 9]),
        (capture, capture + " tab", [44, 92, 8, 33], [68, 6]),
        (capture, capture + " New terminal", [44, 131, 9, 32], [44, 9]),
        (capture, capture + " room", [44, 602.5, 8, 32], [44, 8]),
        (capture, capture + " title", [466, 7.5, 8, 28], [489, 6]),
    ]
}
for capture in (startRoomOnly ? [] : ["toggle-hide", "toggle-hide-repeat"]) {
    samples.append((capture, capture + " title", [466, 7.5, 8, 28], [489, 6]))
}
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
if appearanceOnly { samples = [] }
var darkSamples = Set<String>()
var sampleColors: [String: [Double]] = [:]
var captureWidths: [String: Double] = [:]
for file in ["shortcut-samples.json", "start-room-samples.json", "start-screen-samples.json", "appearance-samples.json", "banner-samples.json", "remove-samples.json", "share-all-samples.json", "process-samples.json", "font-samples.json", "color-samples.json"] {
    let path = output + "/" + file
    if !FileManager.default.fileExists(atPath: path) { continue }
    let rows = try! JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: path))) as! [[String: Any]]
    for row in rows {
        if row["dark_ink"] as? Bool == true { darkSamples.insert(row["name"] as! String) }
        if let width = row["logical_width"] as? Double { captureWidths[row["capture"] as! String] = width }
        if let color = row["color"] as? [Double] { sampleColors[row["name"] as! String] = color.map { $0 / 255 } }
        let box = row["box"] as! [Double]
        samples.append((row["capture"] as! String, row["name"] as! String, box, row["probe"] as! [Double]))
    }
}
if startRoomOnly { precondition(!samples.isEmpty, "start room pixel samples are missing") }
for (capture, name, box, probe) in samples {
    let image = NSBitmapImageRep(data: try! Data(contentsOf: URL(fileURLWithPath: output + "/" + capture + ".png")))!
    let scale = Double(image.pixelsWide) / (captureWidths[capture] ?? 960)
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
                if darkSamples.contains(name) { return contrast > 8.0 / 255 }
                if capture.hasPrefix("colors-") { return contrast > 8.0 / 255 }
                if let foreground = sampleColors[name] {
                    let dominant = foreground.indices.max { foreground[$0] < foreground[$1] }!
                    let other = channels.enumerated().filter { $0.offset != dominant }.map { $0.element }.max()!
                    return contrast > 8.0 / 255 && channels[dominant] - other > 8.0 / 255
                }
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
    let overshoot = probe.count > 2 ? Int((probe[2] * scale).rounded(.up)) : 0
    let bottom = height - stem.last! - 1 + overshoot
    let difference = abs(top - bottom)
    table += "\(name) | \(scale) | \(top) | \(bottom) | \(difference)\n"
    if difference > 1 { failures += 1 }
    let left = min(x, stemX)
    let right = max(x + width, stemX + stemWidth)
    let crop = image.cgImage!.cropping(to: CGRect(x: left, y: y, width: right - left, height: height))!
    try! NSBitmapImageRep(cgImage: crop).representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: output + "/crop-" + name.replacingOccurrences(of: " ", with: "-") + ".png"))
}
for state in (appearanceOnly ? [] : sidebarStates) {
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
if ProcessInfo.processInfo.environment["FONT_QA"] != nil {
    let grid = try! JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: output + "/font-grid.json"))) as! [String: Any]
    let image = NSBitmapImageRep(data: try! Data(contentsOf: URL(fileURLWithPath: output + "/font.png")))!
    let scale = Double(image.pixelsWide) / 960
    let x = grid["x"] as! Double * scale
    let y = grid["y"] as! Double * scale
    let w = grid["width"] as! Double * scale
    let h = grid["height"] as! Double * scale
    func ink(_ x: Int, _ y: Int) -> Bool {
        let color = image.colorAt(x: x, y: y)!.usingColorSpace(.deviceRGB)!
        return color.redComponent > 0.2 && abs(color.redComponent - color.greenComponent) < 0.02
    }
    var offsets: [Int] = []
    for col in [0, 1, 2, 3, 5, 6, 7, 8] {
        let left = Int((x + Double(col) * w).rounded())
        let right = Int((x + Double(col + 1) * w).rounded())
        let columns = (left..<right).filter { column in
            (Int(y.rounded())..<Int((y + h).rounded())).contains { ink(column, $0) }
        }
        precondition(!columns.isEmpty, "missing terminal capital")
        offsets.append(columns.first! - left)
    }
    let aligned = offsets.max()! - offsets.min()! <= 1
    table += "Terminal cell alignment | \(scale) | offsets \(offsets) | aligned \(aligned)\n"
    if !aligned { failures += 1 }
    let horizontalY = Int((y + 3.5 * h).rounded(.down))
    let horizontal = (Int((x + 0.5 * w).rounded(.down))..<Int((x + 4.5 * w).rounded(.down))).allSatisfy { column in
        (-1...1).contains { ink(column, horizontalY + $0) }
    }
    let verticalX = Int((x + 2.5 * w).rounded(.down))
    let vertical = (Int((y + 3.5 * h).rounded(.down))..<Int((y + 5.5 * h).rounded(.down))).allSatisfy { row in
        (-1...1).contains { ink(verticalX + $0, row) }
    }
    let strokeBand = Int(ceil(w / 8)) + 2
    let strokes = (0..<5).map { col in
        let left = (x + Double(col) * w).rounded()
        let right = (x + Double(col + 1) * w).rounded()
        let probeX = Int((left + (right - left) * (col == 4 ? 0.25 : 0.75)).rounded(.down))
        return (-strokeBand...strokeBand).filter { ink(probeX, horizontalY + $0) }.count
    }
    let uniform = Set(strokes).count == 1 && strokes[0] > 0
    table += "Terminal box strokes | \(scale) | widths \(strokes) | uniform \(uniform)\n"
    if !uniform { failures += 1 }
    table += "Terminal box joins | \(scale) | horizontal \(horizontal) | vertical \(vertical)\n"
    if !horizontal || !vertical { failures += 1 }
    let crop = image.cgImage!.cropping(to: CGRect(x: x - w, y: y - h, width: 12 * w, height: 8 * h))!
    try! NSBitmapImageRep(cgImage: crop).representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: output + "/crop-terminal.png"))
}
try! table.write(toFile: output + "/measurements.txt", atomically: true, encoding: .utf8)
let checkCount = samples.count + (appearanceOnly ? 0 : sidebarStates.count) + (ProcessInfo.processInfo.environment["FONT_QA"] == nil ? 0 : 3)
print(table + "Failed checks: \(failures) / \(checkCount)")
exit(failures == 0 ? 0 : 1)
