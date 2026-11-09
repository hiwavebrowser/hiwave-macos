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
//   hwdrive windows  <pid>                  (every ordinary window, on screen or not)
//   hwdrive find     <pid> <text>|role:<AXRole>  (elements by their accessible text or role)
//   hwdrive activate <pid>
//   hwdrive move     <pid> <x> <y>
//   hwdrive click    <pid> <x> <y>
//   hwdrive press    <pid> <x> <y>          (button down, no release)
//   hwdrive release  <pid> <x> <y>
//   hwdrive dblclick <pid> <x> <y>          (two clicks, click counts 1 and 2)
//   hwdrive rightclick   <pid> <x> <y>      (secondary button down and up)
//   hwdrive rightpress   <pid> <x> <y>      (secondary button down only)
//   hwdrive drag     <pid> <x0> <y0> <x1> <y1> (press, dragged moves, release)
//   hwdrive menus    <pid>                  (the process's on-screen windows above layer 0)
//   hwdrive cursor                          (which AppKit cursor is showing)
//   hwdrive scroll   <pid> <x> <y> <dx> <dy> (pixels; dy < 0 moves the page down)
//   hwdrive key      <pid> <keycode> [cmd|shift|alt|ctrl ...]
//   hwdrive menu     <pid> <menu title> <item title>   (press a menu bar item)
//   hwdrive type     <pid> <text>
//   hwdrive resize   <pid> <w> <h>      (returns once the window server shows the size: ack_ms,
//                                        ax_ms, req_epoch; on an AX error ack_ms is null and
//                                        `window` is the read from before the request)
//   hwdrive windows  <pid>                  (every ordinary window, on screen or not)
//
// x, y are points from the top-left corner of the window's frame (title bar
// included). Every command prints one JSON object.
//
// Exit codes: 0 done, 2 usage or no window, 3 a grant is missing, 4 the
// screen is locked (events would go to the login window), 5 refused: the
// process is not frontmost or another app's window is over the point.
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

func mouse(_ type: CGEventType, _ p: CGPoint, clicks: Int64 = 1) {
    let right = type == .rightMouseDown || type == .rightMouseUp || type == .rightMouseDragged
    let e = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: p,
                    mouseButton: right ? .right : .left)
    if [.leftMouseDown, .leftMouseUp, .rightMouseDown, .rightMouseUp].contains(type) {
        e?.setIntegerValueField(.mouseEventClickState, value: clicks)
    }
    post(e)
}

/// The system cursors a page's `cursor` maps to (CH2.E4), by name.
func knownCursors() -> [(String, NSCursor)] {
    return [("arrow", .arrow), ("pointingHand", .pointingHand), ("IBeam", .iBeam),
            ("resizeLeftRight", .resizeLeftRight), ("resizeUpDown", .resizeUpDown),
            ("operationNotAllowed", .operationNotAllowed), ("openHand", .openHand),
            ("closedHand", .closedHand), ("crosshair", .crosshair)]
}

func num(_ s: String) -> Double {
    guard let v = Double(s) else { emit(["error": "not a number: \(s)"], code: 2) }
    return v
}

let args = Array(CommandLine.arguments.dropFirst())
guard let cmd = args.first else { emit(["error": "usage: hwdrive <command> ..."], code: 2) }

if cmd == "preflight" { emit(preflight()) }

// The cursor on screen, matched against the system cursors by image and
// hot spot. Needs no grant.
if cmd == "cursor" {
    // The arrow and the I-beam have no image until AppKit is set up.
    _ = NSApplication.shared
    guard let now = NSCursor.currentSystem else { emit(["error": "no current system cursor"], code: 2) }
    let tiff = now.image.tiffRepresentation
    var name = "unknown"
    for (n, c) in knownCursors() where c.hotSpot == now.hotSpot && c.image.tiffRepresentation == tiff {
        name = n
        break
    }
    emit(["cursor": name, "hot_spot": [now.hotSpot.x, now.hotSpot.y],
          "size": [now.image.size.width, now.image.size.height]])
}

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
    let p = CGPoint(x: w.frame.origin.x + num(args[at]), y: w.frame.origin.y + num(args[at + 1]))
    requireOwnWindowAt(p)
    return p
}

/// A HID event goes to whatever window is on top at its point, not to a
/// pid (CH3.A3b: the flaky `a3_dropdown_over_page` click landed in another
/// app's window over HiWave's, and HiWave saw no event at all). Refuse,
/// before posting anything, unless the process is frontmost and the topmost
/// visible window at `p` (below the menu bar and Dock layers) is its own.
func requireOwnWindowAt(_ p: CGPoint) {
    if NSWorkspace.shared.frontmostApplication?.processIdentifier != pid {
        emit(["error": "pid \(pid) is not frontmost; no event posted",
              "frontmost": Int(NSWorkspace.shared.frontmostApplication?.processIdentifier ?? 0)], code: 5)
    }
    let all = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
    for w in all {  // front to back
        let layer = w[kCGWindowLayer as String] as? Int ?? 0
        let alpha = w[kCGWindowAlpha as String] as? Double ?? 1
        guard layer < 20, alpha > 0,
              let b = w[kCGWindowBounds as String] as? NSDictionary,
              let frame = CGRect(dictionaryRepresentation: b), frame.contains(p)
        else { continue }
        let owner = (w[kCGWindowOwnerPID as String] as? Int).map({ pid_t($0) }) ?? 0
        if owner != pid {
            emit(["error": "another process's window is over the point; no event posted",
                  "owner": Int(owner), "owner_name": w[kCGWindowOwnerName as String] as? String ?? "",
                  "at": [p.x, p.y]], code: 5)
        }
        return
    }
    emit(["error": "no window at the point; no event posted", "at": [p.x, p.y]], code: 5)
}

/// Keys go to the key window of the frontmost app: refuse unless it is ours.
func requireFrontmost() {
    if NSWorkspace.shared.frontmostApplication?.processIdentifier != pid {
        emit(["error": "pid \(pid) is not frontmost; no key posted",
              "frontmost": Int(NSWorkspace.shared.frontmostApplication?.processIdentifier ?? 0)], code: 5)
    }
}

switch cmd {
case "window":
    emit(windowJSON(requireWindow(pid)))

case "windows":
    // Every ordinary (layer 0) window the process owns, largest first: a
    // second window (settings) is found here, and clicked at through the
    // main window's coordinates (its offset from the main window's origin).
    let all = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] ?? []
    var list: [(id: CGWindowID, frame: CGRect, onScreen: Bool)] = []
    for w in all {
        guard (w[kCGWindowOwnerPID as String] as? Int).map({ pid_t($0) }) == pid,
              (w[kCGWindowLayer as String] as? Int) == 0,
              let b = w[kCGWindowBounds as String] as? NSDictionary,
              let frame = CGRect(dictionaryRepresentation: b),
              let id = w[kCGWindowNumber as String] as? Int
        else { continue }
        list.append((CGWindowID(id), frame, (w[kCGWindowIsOnscreen as String] as? Bool) ?? false))
    }
    list.sort { $0.frame.width * $0.frame.height > $1.frame.width * $1.frame.height }
    emit(["windows": list.map(windowJSON)])

case "find":
    // Where elements are, found through Accessibility, depth first over every
    // window of the process. `find <pid> <text>`: the first element whose
    // title, value or description is exactly <text>, scrolled into view first
    // when it offers that. `find <pid> role:<AXRole>`: every element of that
    // role (no scrolling), with its value. Boxes are in the main window's
    // coordinates (the ones `click` takes), so a second window's element is
    // clicked at directly. This only finds; clicks stay real HID events.
    guard args.count >= 3 else { emit(["error": "find needs text or role:<AXRole>"], code: 2) }
    if !AXIsProcessTrusted() {
        emit(["error": "no Accessibility grant (System Settings > Privacy & Security > Accessibility)"], code: 3)
    }
    let query = args[2]
    let role = query.hasPrefix("role:") ? String(query.dropFirst(5)) : nil
    func attr(_ e: AXUIElement, _ name: String) -> CFTypeRef? {
        var v: CFTypeRef?
        return AXUIElementCopyAttributeValue(e, name as CFString, &v) == .success ? v : nil
    }
    func matches(_ e: AXUIElement) -> Bool {
        if let role = role { return (attr(e, kAXRoleAttribute as String) as? String) == role }
        for name in [kAXTitleAttribute, kAXValueAttribute, kAXDescriptionAttribute] {
            if let s = attr(e, name as String) as? String, s == query { return true }
        }
        return false
    }
    var seen = 0
    var hits: [AXUIElement] = []
    func search(_ e: AXUIElement, _ depth: Int) {
        seen += 1
        if seen > 20_000 || depth > 60 || (role == nil && !hits.isEmpty) { return }
        if matches(e) { hits.append(e); if role == nil { return } }
        for child in (attr(e, kAXChildrenAttribute as String) as? [AXUIElement]) ?? [] {
            search(child, depth + 1)
        }
    }
    let app = AXUIElementCreateApplication(pid)
    // A web view builds its accessibility tree only once an assistive
    // client says it is there; ask, then look again for up to 3 s.
    AXUIElementSetAttributeValue(app, "AXEnhancedUserInterface" as CFString, kCFBooleanTrue)
    AXUIElementSetAttributeValue(app, "AXManualAccessibility" as CFString, kCFBooleanTrue)
    for _ in 0..<15 {
        seen = 0
        hits = []
        for win in (attr(app, kAXWindowsAttribute as String) as? [AXUIElement]) ?? [] { search(win, 0) }
        if !hits.isEmpty { break }
        usleep(200_000)
    }
    if hits.isEmpty { emit(["error": "nothing matches \(query)", "searched": seen], code: 2) }
    if role == nil {
        AXUIElementPerformAction(hits[0], "AXScrollToVisible" as CFString)
        usleep(300_000)
    }
    let origin = requireWindow(pid).frame.origin
    func describe(_ el: AXUIElement) -> [String: Any] {
        var pos = CGPoint.zero, size = CGSize.zero
        if let v = attr(el, kAXPositionAttribute as String) { AXValueGetValue(v as! AXValue, .cgPoint, &pos) }
        if let v = attr(el, kAXSizeAttribute as String) { AXValueGetValue(v as! AXValue, .cgSize, &size) }
        var out: [String: Any] = ["role": (attr(el, kAXRoleAttribute as String) as? String) ?? "",
                                  "box": [pos.x - origin.x, pos.y - origin.y, size.width, size.height]]
        if let n = attr(el, kAXValueAttribute as String) as? NSNumber { out["value"] = n.intValue }
        return out
    }
    emit(role == nil ? describe(hits[0]) : ["found": hits.prefix(50).map(describe)])

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

case "dblclick":
    requireInput()
    let p = point(2)
    mouse(.mouseMoved, p)
    for n in [Int64(1), 2] {
        mouse(.leftMouseDown, p, clicks: n)
        mouse(.leftMouseUp, p, clicks: n)
    }
    emit([cmd: [p.x, p.y]])

case "rightclick", "rightpress":
    requireInput()
    let p = point(2)
    mouse(.mouseMoved, p)
    mouse(.rightMouseDown, p)
    if cmd == "rightclick" { mouse(.rightMouseUp, p) }
    emit([cmd: [p.x, p.y]])

case "drag":
    requireInput()
    guard args.count >= 6 else { emit(["error": "drag needs x0 y0 x1 y1"], code: 2) }
    let a = point(2), b = point(4)
    mouse(.mouseMoved, a)
    mouse(.leftMouseDown, a)
    for i in 1...8 {
        let t = Double(i) / 8
        mouse(.leftMouseDragged, CGPoint(x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t))
    }
    mouse(.leftMouseUp, b)
    emit([cmd: [[a.x, a.y], [b.x, b.y]]])

case "menus":
    let all = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
    var layers: [Int] = []
    for w in all where (w[kCGWindowOwnerPID as String] as? Int).map({ pid_t($0) }) == pid {
        let layer = w[kCGWindowLayer as String] as? Int ?? 0
        if layer != 0 { layers.append(layer) }
    }
    emit(["count": layers.count, "layers": layers])

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
    requireFrontmost()
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
    requireFrontmost()
    guard args.count >= 3 else { emit(["error": "type needs text"], code: 2) }
    for ch in args[2] {
        let units = Array(String(ch).utf16)
        for down in [true, false] {
            let e = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: down)
            // No modifiers: an event with no source takes the session's
            // flags, and after a `key ... cmd` those still said Command, so
            // typed text arrived as Cmd chords (CH3.A2, 2026-11-05).
            e?.flags = []
            e?.keyboardSetUnicodeString(stringLength: units.count, unicodeString: units)
            post(e)
        }
    }
    emit(["typed": args[2].count])

case "menu":
    // Press an item of the app's menu bar through Accessibility, as a
    // click on it does: muda's predefined Edit items send their action
    // (copy:, paste:, ...) to the first responder (CH2.E3).
    guard args.count >= 4 else { emit(["error": "menu needs a menu title and an item title"], code: 2) }
    if !AXIsProcessTrusted() {
        emit(["error": "no Accessibility grant (System Settings > Privacy & Security > Accessibility)"], code: 3)
    }
    func children(_ e: AXUIElement) -> [AXUIElement] {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(e, kAXChildrenAttribute as CFString, &value) == .success else { return [] }
        return value as? [AXUIElement] ?? []
    }
    func title(_ e: AXUIElement) -> String {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(e, kAXTitleAttribute as CFString, &value) == .success else { return "" }
        return value as? String ?? ""
    }
    let app = AXUIElementCreateApplication(pid)
    var barRef: CFTypeRef?
    guard AXUIElementCopyAttributeValue(app, kAXMenuBarAttribute as CFString, &barRef) == .success,
          let barValue = barRef
    else { emit(["error": "no menu bar for pid \(pid)"], code: 2) }
    let bar = barValue as! AXUIElement
    guard let top = children(bar).first(where: { title($0) == args[2] }) else {
        emit(["error": "no menu \(args[2])", "menus": children(bar).map(title)], code: 2)
    }
    let items = children(top).flatMap(children)
    guard let item = items.first(where: { title($0) == args[3] }) else {
        emit(["error": "no item \(args[3]) in \(args[2])", "items": items.map(title)], code: 2)
    }
    let err = AXUIElementPerformAction(item, kAXPressAction as CFString)
    usleep(200_000)
    emit(["menu": args[2], "item": args[3], "ax_error": Int(err.rawValue)], code: err == .success ? 0 : 2)

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
    // The acknowledgement is timed here, not by the caller: spawning this
    // process and finding the window cost tens of ms that are not the app's.
    // ax_ms is the AX set round trip (the app's main thread serves it);
    // bounds_ms is request to the window server showing the asked size,
    // polled every 2 ms for up to 2 s. ack_ms is bounds_ms, or null if the
    // window never reached the size in 2 s. req_epoch is the wall-clock
    // time of the request (Unix seconds), the clock the app log's UTC
    // stamps use, so a caller can time the app's own lines from the request
    // rather than from before this process was spawned. On an AX error
    // there is no poll: ack_ms is null and `window` is the pre-request read.
    let reqEpoch = Date().timeIntervalSince1970
    let t0 = DispatchTime.now().uptimeNanoseconds
    let ms = { (t: UInt64) -> Double in (Double(t - t0) / 1e4).rounded() / 100 }
    let err = AXUIElementSetAttributeValue(win, kAXSizeAttribute as CFString, axSize)
    let axMs = ms(DispatchTime.now().uptimeNanoseconds)
    var boundsMs: Double? = nil
    var last = mainWindow(pid)
    if err == .success {
        let deadline = t0 + 2_000_000_000
        while true {
            if let w = last, abs(w.frame.width - size.width) < 1, abs(w.frame.height - size.height) < 1 {
                boundsMs = ms(DispatchTime.now().uptimeNanoseconds)
                break
            }
            if DispatchTime.now().uptimeNanoseconds >= deadline { break }
            usleep(2_000)
            last = mainWindow(pid)
        }
    }
    var out: [String: Any] = [
        "ax_error": Int(err.rawValue), "ax_ms": axMs, "req_epoch": reqEpoch,
        "bounds_ms": boundsMs ?? NSNull(), "ack_ms": boundsMs ?? NSNull(),
    ]
    if let w = last { out["window"] = windowJSON(w) }
    emit(out, code: err == .success ? 0 : 2)

default:
    emit(["error": "unknown command \(cmd)"], code: 2)
}
