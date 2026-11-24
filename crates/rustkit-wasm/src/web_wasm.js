// WebAssembly's JS half (WASM W1 phase 1; docs/design/WASM-W1.md). Evaluated
// once per realm by rustkit-wasm's `install`, which calls the function this
// script evaluates to with the realm's natives: { validate, compile, reflect,
// instantiate, call }. Nothing of them is left on the global object.
//
// The shape follows Chrome for Testing 148
// (docs/diagnostics/2026-11-24/wasm_phase1_cft148.json): constructors of
// length 1, `new` required, frozen null-prototype exports, an exported
// function named by its function index, errors as CompileError / LinkError /
// RuntimeError (subclasses of Error).
(function (N) {
    'use strict';
    var MARK = '\u0001wasm\u0001';
    var moduleIds = new WeakMap();
    // WebAssembly.Memory objects: id <-> object (an exported memory that was
    // imported is the same object, as in Chrome).
    var memoryIds = new WeakMap(), memoriesById = [];
    // Exported wasm functions by id (one object per function: Chrome's
    // table.get(i) === exports.f), and the id of each such object.
    var funcsById = [], funcIds = new WeakMap();
    var tableIds = new WeakMap(), tablesById = [], globalIds = new WeakMap(), globalsById = [];

    function defineError(name) {
        var E = function (message) {
            if (!new.target) return new E(message);
            var e = Reflect.construct(Error, message === undefined ? [] : [message], new.target);
            return e;
        };
        Object.defineProperty(E, 'name', { value: name });
        E.prototype = Object.create(Error.prototype, {
            constructor: { value: E, writable: true, configurable: true },
            name: { value: name, writable: true, configurable: true },
            message: { value: '', writable: true, configurable: true }
        });
        Object.setPrototypeOf(E, Error);
        return E;
    }
    var CompileError = defineError('CompileError');
    var LinkError = defineError('LinkError');
    var RuntimeError = defineError('RuntimeError');
    var CLASSES = { CompileError: CompileError, LinkError: LinkError, RuntimeError: RuntimeError };

    // A native's error string ("\u0001wasm\u0001<Class>\u0001<message>") as its class.
    function convert(e) {
        if (typeof e === 'string' && e.indexOf(MARK) === 0) {
            var rest = e.slice(MARK.length), cut = rest.indexOf('\u0001');
            return new CLASSES[rest.slice(0, cut)](rest.slice(cut + 1));
        }
        return e;
    }
    N.setConvert(convert);
    function guard(f) {
        try { return f(); } catch (e) { throw convert(e); }
    }

    // A BufferSource as a copy of its bytes in a fresh ArrayBuffer.
    function bytes(source, who) {
        if (source instanceof ArrayBuffer) return source.slice(0);
        if (ArrayBuffer.isView(source)) {
            return source.buffer.slice(source.byteOffset, source.byteOffset + source.byteLength);
        }
        throw new TypeError(who + ': Argument 0 must be a buffer source');
    }

    function requireNew(target, who) {
        if (!target) throw new TypeError('WebAssembly.' + who + '(): WebAssembly.' + who + " must be invoked with 'new'");
    }

    function Module(source) {
        requireNew(new.target, 'Module');
        var id = guard(function () { return N.compile(bytes(source, 'WebAssembly.Module()')); });
        moduleIds.set(this, id);
    }
    function idOf(module, who) {
        if (!moduleIds.has(module)) throw new TypeError(who + ': Argument 0 must be a WebAssembly.Module');
        return moduleIds.get(module);
    }
    Object.defineProperty(Module.prototype, Symbol.toStringTag, { value: 'WebAssembly.Module', configurable: true });
    Module.exports = function exports(module) {
        return N.reflect(idOf(module, 'WebAssembly.Module.exports()'), false).map(function (r) {
            return { name: r[0], kind: r[1] };
        });
    };
    Module.imports = function imports(module) {
        return N.reflect(idOf(module, 'WebAssembly.Module.imports()'), true).map(function (r) {
            return { module: r[0], name: r[1], kind: r[2] };
        });
    };
    Module.customSections = function customSections(module, name) {
        idOf(module, 'WebAssembly.Module.customSections()');
        // W1 phase 1: custom sections are not read yet (stated).
        return [];
    };

    function exportFor(funcId, arity, index) {
        var f = funcsById[funcId];
        if (!f) {
            f = makeExport(funcId, arity, index === undefined ? funcId : index);
            funcsById[funcId] = f;
            funcIds.set(f, funcId);
        }
        return f;
    }
    function makeExport(funcId, arity, index) {
        // An arrow function: no prototype and not a constructor, as an
        // exported function is in Chrome.
        // A native function object (lib.rs makeExport): one frame per call,
        // not a JS wrapper around a native.
        return N.makeExport(funcId, arity, String(index));
    }

    // length 1, as in Chrome: the import object is read from arguments.
    // ToIndex-style read of a descriptor field: undefined stays undefined.
    function pages(value, who, field) {
        if (value === undefined) return undefined;
        var n = Number(value);
        if (!isFinite(n) || n < 0 || n > 65536) {
            throw new TypeError(who + ': Property \'' + field + '\': value ' + value + ' is above the upper bound 65536');
        }
        return Math.floor(n);
    }

    function Memory(descriptor) {
        requireNew(new.target, 'Memory');
        if (descriptor === null || typeof descriptor !== 'object') {
            throw new TypeError('WebAssembly.Memory(): Argument 0 must be a memory descriptor');
        }
        if (descriptor.shared) throw new TypeError('WebAssembly.Memory(): shared memory is not supported (WASM W1)');
        var initial = pages(descriptor.initial, 'WebAssembly.Memory()', 'initial');
        if (initial === undefined) throw new TypeError("WebAssembly.Memory(): Property 'initial' is required");
        var maximum = pages(descriptor.maximum, 'WebAssembly.Memory()', 'maximum');
        if (maximum !== undefined && maximum < initial) {
            throw new RangeError("WebAssembly.Memory(): Property 'maximum': value " + maximum + ' is below the lower bound ' + initial);
        }
        var made = guard(function () { return N.memoryNew(initial, maximum === undefined ? -1 : maximum); });
        bindMemory(this, made[0], made[1]);
    }
    function bindMemory(object, id, buffer) {
        memoryIds.set(object, id);
        memoriesById[id] = object;
        Object.defineProperty(object, '__rkBuffer', { value: buffer });
    }
    // A module-defined memory, exported: a Memory whose buffer is a stated W1 limit.
    function exportedMemory(id) {
        if (memoriesById[id]) return memoriesById[id];
        var m = Object.create(Memory.prototype);
        bindMemory(m, id, null);
        return m;
    }
    function memId(m, who) {
        if (!memoryIds.has(m)) throw new TypeError(who + ': Receiver is not a WebAssembly.Memory');
        return memoryIds.get(m);
    }
    Object.defineProperty(Memory.prototype, 'buffer', {
        get: function () {
            memId(this, 'WebAssembly.Memory.buffer');
            if (this.__rkBuffer === null) {
                throw new TypeError('WebAssembly.Memory.buffer: a module-defined memory has no JS buffer in WASM W1 (W2)');
            }
            return this.__rkBuffer;
        },
        configurable: true
    });
    Object.defineProperty(Memory.prototype, 'grow', {
        value: function grow(delta) {
            var id = memId(this, 'WebAssembly.Memory.grow()');
            return guard(function () { return N.memoryGrow(id, delta); });
        },
        writable: true, configurable: true
    });
    Object.defineProperty(Memory.prototype, Symbol.toStringTag, { value: 'WebAssembly.Memory', configurable: true });

    // ---- Table (funcref in W1 phase 3; externref follows)
    function Table(descriptor) {
        requireNew(new.target, 'Table');
        if (descriptor === null || typeof descriptor !== 'object') {
            throw new TypeError('WebAssembly.Table(): Argument 0 must be a table descriptor');
        }
        var element = String(descriptor.element);
        if (element !== 'anyfunc' && element !== 'funcref') {
            throw new TypeError("WebAssembly.Table(): Descriptor property 'element' must be a WebAssembly reference type");
        }
        var initial = pages(descriptor.initial, 'WebAssembly.Table()', 'initial');
        if (initial === undefined) throw new TypeError("WebAssembly.Table(): Property 'initial' is required");
        var maximum = pages(descriptor.maximum, 'WebAssembly.Table()', 'maximum');
        if (maximum !== undefined && maximum < initial) {
            throw new RangeError("WebAssembly.Table(): Property 'maximum': value " + maximum + ' is below the lower bound ' + initial);
        }
        var id = guard(function () { return N.tableNew(initial, maximum === undefined ? -1 : maximum); });
        tableIds.set(this, id); tablesById[id] = this;
    }
    function tableFor(id) {
        if (tablesById[id]) return tablesById[id];
        var t = Object.create(Table.prototype);
        tableIds.set(t, id); tablesById[id] = t;
        return t;
    }
    function tableId(t, who) {
        if (!tableIds.has(t)) throw new TypeError(who + ': Receiver is not a WebAssembly.Table');
        return tableIds.get(t);
    }
    function fromFuncref(r) { return r === null ? null : exportFor(r[0], r[1]); }
    function toFuncref(v, who) {
        if (v === null || v === undefined) return -1;
        if (typeof v === 'function' && funcIds.has(v)) return funcIds.get(v);
        throw new TypeError(who + ': Argument 1 is invalid for table: function-typed object expected');
    }
    Object.defineProperties(Table.prototype, {
        length: { get: function () { return N.tableOp(tableId(this, 'WebAssembly.Table.length'), 'length'); }, configurable: true },
        get: { value: function get(index) {
            var id = tableId(this, 'WebAssembly.Table.get()');
            return fromFuncref(guard(function () { return N.tableOp(id, 'get', index); }));
        }, writable: true, configurable: true },
        set: { value: function set(index, value) {
            var id = tableId(this, 'WebAssembly.Table.set()'), ref = toFuncref(value, 'WebAssembly.Table.set()');
            guard(function () { return N.tableOp(id, 'set', index, ref); });
        }, writable: true, configurable: true },
        grow: { value: function grow(delta) {
            var id = tableId(this, 'WebAssembly.Table.grow()');
            return guard(function () { return N.tableOp(id, 'grow', delta); });
        }, writable: true, configurable: true }
    });
    Object.defineProperty(Table.prototype, Symbol.toStringTag, { value: 'WebAssembly.Table', configurable: true });

    // ---- Global
    function Global(descriptor, value) {
        requireNew(new.target, 'Global');
        if (descriptor === null || typeof descriptor !== 'object') {
            throw new TypeError('WebAssembly.Global(): Argument 0 must be a global descriptor');
        }
        var type = String(descriptor.value);
        if (['i32', 'i64', 'f32', 'f64'].indexOf(type) < 0) {
            throw new TypeError("WebAssembly.Global(): Descriptor property 'value' must be a WebAssembly type");
        }
        var id = guard(function () { return N.globalNew(type, !!descriptor.mutable, value); });
        globalIds.set(this, id); globalsById[id] = this;
    }
    function globalFor(id) {
        if (globalsById[id]) return globalsById[id];
        var g = Object.create(Global.prototype);
        globalIds.set(g, id); globalsById[id] = g;
        return g;
    }
    function globalId(g, who) {
        if (!globalIds.has(g)) throw new TypeError(who + ': Receiver is not a WebAssembly.Global');
        return globalIds.get(g);
    }
    Object.defineProperty(Global, 'length', { value: 1 });
    Object.defineProperties(Global.prototype, {
        value: {
            get: function () { var id = globalId(this, 'get WebAssembly.Global.value'); return N.globalOp(id, 'get'); },
            set: function (v) { var id = globalId(this, 'set WebAssembly.Global.value'); guard(function () { return N.globalOp(id, 'set', v); }); },
            configurable: true
        },
        valueOf: { value: function valueOf() { return N.globalOp(globalId(this, 'WebAssembly.Global.valueOf()'), 'get'); }, writable: true, configurable: true }
    });
    Object.defineProperty(Global.prototype, Symbol.toStringTag, { value: 'WebAssembly.Global', configurable: true });

    function Instance(module) {
        requireNew(new.target, 'Instance');
        var importObject = arguments[1];
        var id = idOf(module, 'WebAssembly.Instance()');
        var wanted = N.reflect(id, true);
        if (wanted.length && (importObject === undefined || importObject === null || typeof importObject !== 'object')) {
            throw new TypeError('WebAssembly.Instance(): Imports argument must be present and must be an object');
        }
        var given = wanted.map(function (w, i) {
            var mod = importObject[w[0]];
            if (mod === null || (typeof mod !== 'object' && typeof mod !== 'function')) {
                throw new TypeError('WebAssembly.Instance(): Import #' + i + ' "' + w[0] + '": module is not an object or function');
            }
            var value = mod[w[1]];
            if (w[2] === 'memory') value = memoryIds.has(value) ? memoryIds.get(value) : -1;
            else if (w[2] === 'table') value = tableIds.has(value) ? tableIds.get(value) : -1;
            else if (w[2] === 'global') value = globalIds.has(value) ? ['G', globalIds.get(value)] : ['V', value];
            return [w[0], w[1], value];
        });
        var rows = guard(function () { return N.instantiate(id, given); });
        var exports = Object.create(null);
        rows.forEach(function (r) {
            if (r[1] === 'memory') exports[r[0]] = exportedMemory(r[2]);
            else if (r[1] === 'table') exports[r[0]] = tableFor(r[2]);
            else if (r[1] === 'global') exports[r[0]] = globalFor(r[2]);
            else exports[r[0]] = exportFor(r[1], r[2], r[3]);
        });
        Object.freeze(exports);
        Object.defineProperty(this, 'exports', { value: exports, enumerable: true, configurable: true });
    }
    Object.defineProperty(Instance.prototype, Symbol.toStringTag, { value: 'WebAssembly.Instance', configurable: true });

    function settle(f) {
        return new Promise(function (resolve, reject) {
            try { resolve(f()); } catch (e) { reject(convert(e)); }
        });
    }

    var WebAssembly = {
        validate: function validate(source) {
            return N.validate(bytes(source, 'WebAssembly.validate()'));
        },
        compile: function compile(source) {
            return settle(function () { return new Module(source); });
        },
        instantiate: function instantiate(source, importObject) {
            return settle(function () {
                if (source instanceof Module) return new Instance(source, importObject);
                var module = new Module(source);
                return { module: module, instance: new Instance(module, importObject) };
            });
        },
        Module: Module,
        Instance: Instance,
        Memory: Memory,
        Table: Table,
        Global: Global,
        CompileError: CompileError,
        LinkError: LinkError,
        RuntimeError: RuntimeError
    };
    ['validate', 'compile', 'instantiate'].forEach(function (k) {
        Object.defineProperty(WebAssembly, k, { enumerable: false });
    });
    ['Module', 'Instance', 'Memory', 'Table', 'Global', 'CompileError', 'LinkError', 'RuntimeError'].forEach(function (k) {
        Object.defineProperty(WebAssembly, k, { enumerable: false });
    });
    Object.defineProperty(WebAssembly, Symbol.toStringTag, { value: 'WebAssembly', configurable: true });
    Object.defineProperty(globalThis, 'WebAssembly', { value: WebAssembly, writable: true, configurable: true, enumerable: false });
})
