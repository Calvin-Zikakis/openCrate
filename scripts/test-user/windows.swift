// windows.swift <owner>: list on-screen windows whose app name contains <owner> (any case):
// window id, app, title, bounds in screen points, whether on screen, and layer.
import CoreGraphics
import Foundation
let wanted = CommandLine.arguments.count > 1 ? CommandLine.arguments[1].lowercased() : ""
let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] ?? []
for window in list {
    let owner = window[kCGWindowOwnerName as String] as? String ?? ""
    guard owner.lowercased().contains(wanted) else { continue }
    let id = window[kCGWindowNumber as String] as? Int ?? 0
    let name = window[kCGWindowName as String] as? String ?? ""
    let bounds = window[kCGWindowBounds as String] as? [String: Any] ?? [:]
    let onscreen = window[kCGWindowIsOnscreen as String] as? Bool ?? false
    let layer = window[kCGWindowLayer as String] as? Int ?? 0
    print(id, owner, "|", name, "|", bounds, "onscreen:", onscreen, "layer:", layer)
}
