// Console interceptor for HiWave Inspector
// This script captures console messages and sends them to the inspector panel
(function() {
    'use strict';

    // Prevent multiple injections
    if (window.__hiwaveConsole) return;

    // Store original console methods
    const originalConsole = {
        log: console.log.bind(console),
        warn: console.warn.bind(console),
        error: console.error.bind(console),
        info: console.info.bind(console),
        debug: console.debug.bind(console),
        clear: console.clear.bind(console),
        table: console.table.bind(console),
        trace: console.trace.bind(console),
        dir: console.dir.bind(console),
        count: console.count.bind(console),
        countReset: console.countReset.bind(console),
        group: console.group.bind(console),
        groupEnd: console.groupEnd.bind(console),
        groupCollapsed: console.groupCollapsed.bind(console),
        time: console.time.bind(console),
        timeEnd: console.timeEnd.bind(console),
        timeLog: console.timeLog.bind(console),
        assert: console.assert.bind(console)
    };

    // Console entry counter for unique IDs
    let consoleEntryId = 0;

    // Serialize value for console output
    function serializeValue(val, depth = 0, seen = new WeakSet()) {
        if (depth > 5) return '...';

        if (val === null) return 'null';
        if (val === undefined) return 'undefined';

        const type = typeof val;

        if (type === 'string') return JSON.stringify(val);
        if (type === 'number' || type === 'boolean') return String(val);
        if (type === 'symbol') return val.toString();
        if (type === 'bigint') return val.toString() + 'n';
        if (type === 'function') {
            const name = val.name || 'anonymous';
            return `[Function: ${name}]`;
        }

        if (type === 'object') {
            // Circular reference check
            if (seen.has(val)) return '[Circular]';
            seen.add(val);

            // DOM Element
            if (val instanceof Element) {
                let str = val.tagName.toLowerCase();
                if (val.id) str += '#' + val.id;
                if (val.className && typeof val.className === 'string') {
                    str += '.' + val.className.split(' ').filter(Boolean).slice(0, 2).join('.');
                }
                return `<${str}>`;
            }

            // Error
            if (val instanceof Error) {
                return `${val.name}: ${val.message}${val.stack ? '\n' + val.stack : ''}`;
            }

            // Date
            if (val instanceof Date) {
                return val.toISOString();
            }

            // RegExp
            if (val instanceof RegExp) {
                return val.toString();
            }

            // Array
            if (Array.isArray(val)) {
                if (val.length === 0) return '[]';
                if (depth > 2) return `[Array(${val.length})]`;
                const items = val.slice(0, 100).map(v => serializeValue(v, depth + 1, seen));
                if (val.length > 100) items.push(`... ${val.length - 100} more items`);
                return '[' + items.join(', ') + ']';
            }

            // Map
            if (val instanceof Map) {
                const entries = [];
                let count = 0;
                for (const [k, v] of val) {
                    if (count++ >= 20) {
                        entries.push(`... ${val.size - 20} more entries`);
                        break;
                    }
                    entries.push(`${serializeValue(k, depth + 1, seen)} => ${serializeValue(v, depth + 1, seen)}`);
                }
                return `Map(${val.size}) {${entries.join(', ')}}`;
            }

            // Set
            if (val instanceof Set) {
                const items = [];
                let count = 0;
                for (const v of val) {
                    if (count++ >= 20) {
                        items.push(`... ${val.size - 20} more items`);
                        break;
                    }
                    items.push(serializeValue(v, depth + 1, seen));
                }
                return `Set(${val.size}) {${items.join(', ')}}`;
            }

            // Promise
            if (val instanceof Promise) {
                return 'Promise { <pending> }';
            }

            // Generic object
            try {
                const keys = Object.keys(val).slice(0, 50);
                if (keys.length === 0) return '{}';
                if (depth > 2) return '{...}';

                const pairs = keys.map(k => {
                    try {
                        return `${k}: ${serializeValue(val[k], depth + 1, seen)}`;
                    } catch (e) {
                        return `${k}: [Error accessing property]`;
                    }
                });

                if (Object.keys(val).length > 50) {
                    pairs.push(`... ${Object.keys(val).length - 50} more properties`);
                }

                const constructor = val.constructor?.name;
                if (constructor && constructor !== 'Object') {
                    return `${constructor} {${pairs.join(', ')}}`;
                }
                return '{' + pairs.join(', ') + '}';
            } catch (e) {
                return '[Object]';
            }
        }

        return String(val);
    }

    // Format console arguments
    function formatConsoleArgs(args) {
        return Array.from(args).map(arg => serializeValue(arg)).join(' ');
    }

    // Get stack trace for console calls
    function getStackTrace() {
        const error = new Error();
        const stack = error.stack || '';
        const lines = stack.split('\n').slice(3); // Skip Error, getStackTrace, and intercepted call

        // Filter out internal lines and format
        return lines
            .filter(line => !line.includes('console_inject.js'))
            .slice(0, 5)
            .map(line => {
                // Parse stack line format: "at functionName (file:line:col)" or "at file:line:col"
                const match = line.match(/at\s+(?:(.+?)\s+\()?(.+?):(\d+):(\d+)\)?/);
                if (match) {
                    return {
                        fn: match[1] || '<anonymous>',
                        file: match[2],
                        line: parseInt(match[3], 10),
                        col: parseInt(match[4], 10)
                    };
                }
                return { raw: line.trim() };
            });
    }

    // Send console message to inspector
    function sendConsoleMessage(level, args, stack) {
        const message = {
            id: ++consoleEntryId,
            level: level,
            message: formatConsoleArgs(args),
            timestamp: Date.now(),
            stack: stack || []
        };

        try {
            if (window.ipc && window.ipc.postMessage) {
                window.ipc.postMessage(JSON.stringify({
                    cmd: 'inspector_console_message',
                    entry: message
                }));
            }
        } catch (e) {
            // Fallback to original console if IPC fails
            originalConsole.error('Failed to send console message to inspector:', e);
        }
    }

    // Intercept console methods
    console.log = function(...args) {
        originalConsole.log(...args);
        sendConsoleMessage('log', args, getStackTrace());
    };

    console.warn = function(...args) {
        originalConsole.warn(...args);
        sendConsoleMessage('warn', args, getStackTrace());
    };

    console.error = function(...args) {
        originalConsole.error(...args);
        sendConsoleMessage('error', args, getStackTrace());
    };

    console.info = function(...args) {
        originalConsole.info(...args);
        sendConsoleMessage('info', args, getStackTrace());
    };

    console.debug = function(...args) {
        originalConsole.debug(...args);
        sendConsoleMessage('debug', args, getStackTrace());
    };

    console.clear = function() {
        originalConsole.clear();
        if (window.ipc && window.ipc.postMessage) {
            window.ipc.postMessage(JSON.stringify({
                cmd: 'inspector_console_clear'
            }));
        }
    };

    console.table = function(data, columns) {
        originalConsole.table(data, columns);
        sendConsoleMessage('table', [data], getStackTrace());
    };

    console.trace = function(...args) {
        originalConsole.trace(...args);
        const stack = getStackTrace();
        const message = args.length > 0 ? formatConsoleArgs(args) + '\n' : '';
        sendConsoleMessage('trace', [message + 'Stack trace:\n' + stack.map(s =>
            s.raw || `  at ${s.fn} (${s.file}:${s.line}:${s.col})`
        ).join('\n')], stack);
    };

    console.dir = function(obj, options) {
        originalConsole.dir(obj, options);
        sendConsoleMessage('dir', [obj], getStackTrace());
    };

    console.assert = function(condition, ...args) {
        originalConsole.assert(condition, ...args);
        if (!condition) {
            sendConsoleMessage('error', ['Assertion failed:', ...args], getStackTrace());
        }
    };

    // Count tracking
    const counts = new Map();
    console.count = function(label = 'default') {
        const count = (counts.get(label) || 0) + 1;
        counts.set(label, count);
        originalConsole.count(label);
        sendConsoleMessage('log', [`${label}: ${count}`], []);
    };

    console.countReset = function(label = 'default') {
        counts.delete(label);
        originalConsole.countReset(label);
    };

    // Timer tracking
    const timers = new Map();
    console.time = function(label = 'default') {
        timers.set(label, performance.now());
        originalConsole.time(label);
    };

    console.timeEnd = function(label = 'default') {
        const start = timers.get(label);
        if (start !== undefined) {
            const duration = performance.now() - start;
            timers.delete(label);
            originalConsole.timeEnd(label);
            sendConsoleMessage('log', [`${label}: ${duration.toFixed(3)}ms`], []);
        }
    };

    console.timeLog = function(label = 'default', ...args) {
        const start = timers.get(label);
        if (start !== undefined) {
            const duration = performance.now() - start;
            originalConsole.timeLog(label, ...args);
            sendConsoleMessage('log', [`${label}: ${duration.toFixed(3)}ms`, ...args], []);
        }
    };

    // Group tracking (visual only - we just log them)
    console.group = function(...args) {
        originalConsole.group(...args);
        sendConsoleMessage('group', args.length > 0 ? args : ['group'], []);
    };

    console.groupCollapsed = function(...args) {
        originalConsole.groupCollapsed(...args);
        sendConsoleMessage('groupCollapsed', args.length > 0 ? args : ['group'], []);
    };

    console.groupEnd = function() {
        originalConsole.groupEnd();
        sendConsoleMessage('groupEnd', [], []);
    };

    // Capture uncaught errors
    window.addEventListener('error', function(event) {
        const stack = [{
            fn: '<global>',
            file: event.filename || 'unknown',
            line: event.lineno || 0,
            col: event.colno || 0
        }];
        sendConsoleMessage('error', [`Uncaught ${event.error?.name || 'Error'}: ${event.message}`], stack);
    });

    window.addEventListener('unhandledrejection', function(event) {
        let message = 'Unhandled Promise Rejection';
        if (event.reason) {
            if (event.reason instanceof Error) {
                message = `Unhandled Promise Rejection: ${event.reason.name}: ${event.reason.message}`;
            } else {
                message = `Unhandled Promise Rejection: ${serializeValue(event.reason)}`;
            }
        }
        sendConsoleMessage('error', [message], []);
    });

    // Mark as initialized
    window.__hiwaveConsole = {
        originalConsole: originalConsole
    };
})();
