// Edit commands (see editing.rs): document.execCommand and
// queryCommandSupported/Enabled, navigator.clipboard, and the user's edit
// commands (`__rkUserEdit`, which DomBindings::user_edit calls).
(function (g) {
    var I = g.__rkEditInternals, EDIT = __rustkit_edit,
        READ = __rustkit_clip_read, WRITE = __rustkit_clip_write;
    delete g.__rkEditInternals;
    delete g.__rustkit_edit;
    delete g.__rustkit_clip_read;
    delete g.__rustkit_clip_write;
    var Document = g.Document;

    function hidden(o, name, value) {
        Object.defineProperty(o, name, { value: value, configurable: true, enumerable: false, writable: true });
    }
    function slot(el) {
        try { var s = I.slotOf(el); } catch (e) { return null; }
        return s.gen === I.gen() ? s.id : null;
    }
    // A command's report (editing.rs), applied to script's view of the
    // control: its selection follows the edit state's.
    function run(el, id, cmd, value, sel, user) {
        var r = EDIT(I.gen(), id, 'run', cmd, value == null ? null : String(value),
                     sel ? sel.start : null, sel ? sel.end : null, user);
        if (typeof r !== 'string') return null;
        r = JSON.parse(r);
        I.setSelection(el, r.start, r.end, r.dir);
        return r;
    }
    function fire(el, type, inputType, data, cancelable) {
        if (type === 'input') hidden(el, '__rkEdited', true);
        var ev = new g.InputEvent(type, {
            bubbles: true, cancelable: !!cancelable, composed: true, view: g,
            inputType: inputType, data: data === undefined ? null : data
        });
        ev.isTrusted = true;
        return el.dispatchEvent(ev);
    }

    // ---- document.execCommand (the Editing API's text-control subset) ----
    var COMMANDS = { copy: 'copy', cut: 'cut', paste: 'paste', selectall: 'selectAll',
                     undo: 'undo', redo: 'redo', inserttext: 'insertText' };
    function name(cmd) { return COMMANDS[String(cmd).toLowerCase()] || null; }
    // The focused text control execCommand acts on.
    function target() {
        var el = document.activeElement;
        if (!el || !/^(input|textarea)$/.test(el.localName)) return null;
        var id = slot(el);
        return id === null ? null : { el: el, id: id };
    }
    hidden(Document.prototype, 'execCommand', function (command, showUI, value) {
        var cmd = name(command), t = target();
        if (!cmd || !t) return false;
        var sel = I.selectionOf(t.el);
        var r = run(t.el, t.id, cmd, value, { start: sel.start, end: sel.end }, false);
        if (!r || !r.handled) return false;
        // Script's commands fire `input`, not `beforeinput` (Input Events §5.1).
        if (r.changed) fire(t.el, 'input', r.inputType, r.data);
        return true;
    });
    hidden(Document.prototype, 'queryCommandSupported', function (command) {
        return name(command) !== null;
    });
    hidden(Document.prototype, 'queryCommandEnabled', function (command) {
        var cmd = name(command), t = target();
        if (!cmd || !t) return false;
        var sel = I.selectionOf(t.el);
        return EDIT(I.gen(), t.id, 'check', cmd, null, sel.start, sel.end, false) === true;
    });

    // ---- the user's command (a shortcut the engine mapped) ----
    // beforeinput (cancelable) for an edit, the edit, then input. Answers
    // the report as JSON, or null (not a text control, or cancelled).
    var INPUT_TYPES = { cut: 'deleteByCut', paste: 'insertFromPaste', undo: 'historyUndo',
                        redo: 'historyRedo', insertText: 'insertText',
                        deleteWordBackward: 'deleteWordBackward', deleteWordForward: 'deleteWordForward' };
    hidden(Document.prototype, '__rkUserEdit', function (id, cmd, value) {
        var el = typeof id === 'number' ? I.wrap(id) : null;
        if (!el) return null;
        var inputType = INPUT_TYPES[cmd];
        if (inputType) {
            if (EDIT(I.gen(), id, 'check', cmd, value, null, null, true) !== true) {
                // Nothing to do: no events, nothing changes.
                return JSON.stringify({ handled: false, changed: false });
            }
            var data = cmd === 'paste' ? READ(true) : cmd === 'insertText' ? String(value) : null;
            if (!fire(el, 'beforeinput', inputType, data, true)) return null;
        }
        var r = run(el, id, cmd, value, null, true);
        if (!r) return null;
        if (r.changed) fire(el, 'input', r.inputType, r.data);
        return JSON.stringify(r);
    });

    // ---- navigator.clipboard (Clipboard API §6, text only) ----
    function Clipboard() { throw new TypeError('Illegal constructor'); }
    Object.defineProperty(Clipboard, 'name', { value: 'Clipboard' });
    if (g.EventTarget) Object.setPrototypeOf(Clipboard.prototype, g.EventTarget.prototype);
    Object.defineProperty(Clipboard.prototype, Symbol.toStringTag, { value: 'Clipboard' });
    hidden(Clipboard.prototype, 'readText', function () {
        return new Promise(function (resolve, reject) {
            var t = READ(false);
            if (t === false) reject(new g.DOMException('Read permission denied.', 'NotAllowedError'));
            else resolve(t);
        });
    });
    hidden(Clipboard.prototype, 'writeText', function (text) {
        if (arguments.length < 1) {
            return Promise.reject(new TypeError("Failed to execute 'writeText' on 'Clipboard': 1 argument required."));
        }
        var s = String(text);
        // Checked now, while the caller's activation is current: writes
        // need a user activation (Clipboard API §6.4.1, as in Chrome).
        var ok = WRITE(s);
        return new Promise(function (resolve, reject) {
            if (ok === false) reject(new g.DOMException('Write permission denied.', 'NotAllowedError'));
            else resolve(undefined);
        });
    });
    hidden(g, 'Clipboard', Clipboard);
    var clipboard = Object.create(Clipboard.prototype);
    Object.defineProperty(g.navigator, 'clipboard', {
        get: function () { return clipboard; }, configurable: true, enumerable: true
    });
})(globalThis);
