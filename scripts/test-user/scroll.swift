// scroll.swift <x> <y> <lines>: move the pointer to (x, y) in screen points and scroll there;
// negative lines scroll down. cliclick cannot scroll, so this posts wheel events itself.
import CoreGraphics
import Foundation
let args = CommandLine.arguments
guard args.count == 4, let x = Double(args[1]), let y = Double(args[2]), let lines = Int32(args[3]) else {
    print("usage: scroll <x> <y> <lines>")
    exit(2)
}
let point = CGPoint(x: x, y: y)
CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: point, mouseButton: .left)?.post(tap: .cghidEventTap)
usleep(100_000)
let step: Int32 = lines < 0 ? -1 : 1
for _ in 0..<abs(lines) {
    CGEvent(scrollWheelEvent2Source: nil, units: .line, wheelCount: 1, wheel1: step, wheel2: 0, wheel3: 0)?.post(tap: .cghidEventTap)
    usleep(15_000)
}
