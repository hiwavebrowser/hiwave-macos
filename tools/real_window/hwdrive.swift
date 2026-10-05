// hwdrive: the input half of the real-window driver (Z2-I2).
//
// Posts real HID events (CGEvent at the HID tap) at a window of a running
// process and resizes it through Accessibility, so the events take the same
// road a person's do: WindowServer -> NSApplication -> tao -> the content
// view. driver.py launches the app, calls this, captures the window and
// reads the logs.
//
//   hwdrive preflight
//   hwdrive request                       (show the system's grant prompts)
//   hwdrive window   <pid>
//   hwdrive activate <pid>
//   hwdrive move     <pid> <x> <y>
//   hwdrive click    <pid> <x> <y>
//   hwdrive press    <pid> <x> <y>          (button down, no release)
//   hwdrive release  <pid> <x> <y>
//   hwdrive scroll   <pid> <x> <y> <dx> <dy> (pixels; dy < 0 moves the page down)
//   hwdrive key      <pid> <keycode> [cmd|shift|alt|ctrl ...]
//   hwdrive type     <pid> <text>
//   hwdrive resize   <pid> <w> <h>
//
// x, y are points from the top-left corner of the window's frame (title bar
// included). Every command prints one JSON object.
//
// Exit codes: 0 done, 2 usage or no window, 3 a grant is missing, 4 the
// screen is locked (events would go to the login window).
//
// Build: swiftc -O -o hwdrive hwdrive.swift

import AppKit
import ApplicationServices
import CoreGraphics
import Foundation

func emit(_ obj: [String: Any], code: Int32 = 0) -> Never {
    let data = try! JSONSerialization.data(withJSONObject: obj, options: [.sortedKeys])
    print(String(data: data, encoding: .utf8)!)
    exit(code)
}

func screenLocked() -> Bool {
    guard let d = CGSessionCopyCurrentDictionary() as? [String: Any] else { return true }
    return (d["CGSSessionScreenIsLocked"] as? Int ?? 0) != 0
}

func preflight() -> [String: Any] {
    let main = CGMainDisplayID()
    var scale = 1.0
    if let mode = CGDisplayCopyDisplayMode(main), mode.width > 0 {
        scale = Double(mode.pixelWidth) / Double(mode.width)
    }
    return [
        "post_events": CGPreflightPostEventAccess(),
        "accessibility": AXIsProcessTrusted(),
        "screen_capture": CGPreflightScreenCaptureAccess(),
        "screen_locked": screenLocked(),
        "display_asleep": CGDisplayIsAsleep(main) != 0,
        "display_points": [CGDisplayPixelsWide(main), CGDisplayPixelsHigh(main)],
        "display_scale": scale,
    ]
}

/// The largest ordinary (layer 0) window the process owns.
func mainWindow(_ pid: pid_t) -> (id: CGWindowID, frame: CGRect, onScreen: Bool)? {
    let all = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] ?? []
    var best: (id: CGWindowID, frame: CGRect, onScreen: Bool)? = nil
    for w in all {
        guard (w[kCGWindowOwnerPID as String] as? Int).map({ pid_t($0) }) == pid,
              (w[kCGWindowLayer as String] as? Int) == 0,
              let b = w[kCGWindowBounds as String] as? NSDictionary,
              let frame = CGRect(dictionaryRepresentation: b),
              let id = w[kCGWindowNumber as String] as? Int
        else { continue }
        let on = (w[kCGWindowIsOnscreen as String] as? Bool) ?? false
        if best == nil || frame.width * frame.height > best!.frame.width * best!.frame.height {
            best = (CGWindowID(id), frame, on)
        }
    }
    return best
}

func windowJSON(_ w: (id: CGWindowID, frame: CGRect, onScreen: Bool)) -> [String: Any] {
    return [
        "id": Int(w.id), "x": w.frame.origin.x, "y": w.frame.origin.y,
        "w": w.frame.width, "h": w.frame.height, "on_screen": w.onScreen,
    ]
}

func requireWindow(_ pid: pid_t) -> (id: CGWindowID, frame: CGRect, onScreen: Bool) {
    guard let w = mainWindow(pid) else { emit(["error": "no window for pid \(pid)"], code: 2) }
    return w
}

/// Input needs the grant and an unlocked screen; without either, CGEventPost
/// returns as if it had worked and nothing arrives.
func requireInput() {
    if !CGPreflightPostEventAccess() {
        emit(["error": "no grant to post events (System Settings > Privacy & Security > Accessibility)"], code: 3)
    }
    if screenLocked() {
        emit(["error": "the screen is locked; events would go to the login window"], code: 4)
    }
}

func post(_ e: CGEvent?) {
    e?.post(tap: .cghidEventTap)
    usleep(30_000)
}

func mouse(_ type: CGEventType, _ p: CGPoint) {
    let e = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: p, mouseButton: .left)
    if type == .leftMouseDown || type == .leftMouseUp {
        e?.setIntegerValueField(.mouseEventClickState, value: 1)
    }
    post(e)
}

func num(_ s: String) -> Double {
    guard let v = Double(s) else { emit(["error": "not a number: \(s)"], code: 2) }
    return v
}

let args = Array(CommandLine.arguments.dropFirst())
guard let cmd = args.first else { emit(["error": "usage: hwdrive <command> ..."], code: 2) }

if cmd == "preflight" { emit(preflight()) }

// Asks macOS for the three grants. The system shows its own prompts and adds
// the program that started this tool to the lists in System Settings, which
// is the reliable way to find out which program that is. Run it by hand, at
// the Mac.
if cmd == "request" {
    let prompt = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
    emit([
        "accessibility": AXIsProcessTrustedWithOptions(prompt),
        "post_events": CGRequestPostEventAccess(),
        "screen_capture": CGRequestScreenCaptureAccess(),
    ])
}

guard args.count >= 2, let pidInt = Int32(args[1]) else { emit(["error": "\(cmd) needs a pid"], code: 2) }
let pid = pid_t(pidInt)

func point(_ at: Int) -> CGPoint {
    guard args.count > at + 1 else { emit(["error": "\(cmd) needs x y"], code: 2) }
    let w = requireWindow(pid)
    return CGPoint(x: w.frame.origin.x + num(args[at]), y: w.frame.origin.y + num(args[at + 1]))
}

switch cmd {
case "window":
    emit(windowJSON(requireWindow(pid)))

case "activate":
    requireInput()
    let ok = NSRunningApplication(processIdentifier: pid)?.activate(options: [.activateAllWindows]) ?? false
    usleep(300_000)
    emit(["activated": ok, "frontmost": NSWorkspace.shared.frontmostApplication?.processIdentifier == pid])

case "move":
    requireInput()
    let p = point(2)
    mouse(.mouseMoved, p)
    emit(["moved": [p.x, p.y]])

case "click", "press", "release":
    requireInput()
    let p = point(2)
    mouse(.mouseMoved, p)
    if cmd != "release" { mouse(.leftMouseDown, p) }
    if cmd != "press" { mouse(.leftMouseUp, p) }
    emit([cmd: [p.x, p.y]])

case "scroll":
    requireInput()
    let p = point(2)
    guard args.count >= 6 else { emit(["error": "scroll needs x y dx dy"], code: 2) }
    // A wheel event goes to the window under the pointer, so put it there.
    mouse(.mouseMoved, p)
    let e = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 2,
                    wheel1: Int32(num(args[5])), wheel2: Int32(num(args[4])), wheel3: 0)
    post(e)
    emit(["scrolled": [num(args[4]), num(args[5])], "at": [p.x, p.y]])

case "key":
    requireInput()
    guard args.count >= 3, let code = UInt16(args[2]) else { emit(["error": "key needs a keycode"], code: 2) }
    var flags: CGEventFlags = []
    for m in args.dropFirst(3) {
        switch m {
        case "cmd": flags.insert(.maskCommand)
        case "shift": flags.insert(.maskShift)
        case "alt": flags.insert(.maskAlternate)
        case "ctrl": flags.insert(.maskControl)
        default: emit(["error": "unknown modifier \(m)"], code: 2)
        }
    }
    for down in [true, false] {
        let e = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: down)
        e?.flags = flags
        post(e)
    }
    emit(["key": Int(code)])

case "type":
    requireInput()
    guard args.count >= 3 else { emit(["error": "type needs text"], code: 2) }
    for ch in args[2] {
        let units = Array(String(ch).utf16)
        for down in [true, false] {
            let e = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: down)
            e?.keyboardSetUnicodeString(stringLength: units.count, unicodeString: units)
            post(e)
        }
    }
    emit(["typed": args[2].count])

case "resize":
    guard args.count >= 4 else { emit(["error": "resize needs w h"], code: 2) }
    if !AXIsProcessTrusted() {
        emit(["error": "no Accessibility grant (System Settings > Privacy & Security > Accessibility)"], code: 3)
    }
    let app = AXUIElementCreateApplication(pid)
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &value) == .success,
          let wins = value as? [AXUIElement], let win = wins.first
    else { emit(["error": "no accessibility window for pid \(pid)"], code: 2) }
    var size = CGSize(width: num(args[2]), height: num(args[3]))
    let axSize = AXValueCreate(.cgSize, &size)!
    let err = AXUIElementSetAttributeValue(win, kAXSizeAttribute as CFString, axSize)
    usleep(400_000)
    var out: [String: Any] = ["ax_error": Int(err.rawValue)]
    if let w = mainWindow(pid) { out["window"] = windowJSON(w) }
    emit(out, code: err == .success ? 0 : 2)

default:
    emit(["error": "unknown command \(cmd)"], code: 2)
}
