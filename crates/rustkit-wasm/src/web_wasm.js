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

    function makeExport(funcId, arity, index) {
        // An arrow function: no prototype and not a constructor, as an
        // exported function is in Chrome.
        var f = (...args) => {
            try { return N.call(funcId, args); } catch (e) { throw convert(e); }
        };
        Object.defineProperty(f, 'name', { value: String(index), configurable: true });
        Object.defineProperty(f, 'length', { value: arity, configurable: true });
        return f;
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
            return [w[0], w[1], value];
        });
        var rows = guard(function () { return N.instantiate(id, given); });
        var exports = Object.create(null);
        rows.forEach(function (r) {
            exports[r[0]] = r[1] === 'memory' ? exportedMemory(r[2]) : makeExport(r[1], r[2], r[3]);
        });
        Object.freeze(exports);
        Object.defineProperty(this, 'exports', { value: exports, enumerable: true, configurable: true });
    }
    Object.defineProperty(Instance.prototype, Symbol.toStringTag, { value: 'WebAssembly.Instance', configurable: true });

    function notYet(name, phase) {
        var F = function (descriptor) {
            requireNew(new.target, name);
            throw new TypeError('WebAssembly.' + name + ' arrives in WASM W1 phase ' + phase);
        };
        Object.defineProperty(F, 'name', { value: name });
        return F;
    }

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
        Table: notYet('Table', 3),
        Global: notYet('Global', 3),
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
