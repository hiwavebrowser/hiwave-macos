// reporter.js: the stress harness's in-page half (tools/stress/run.py).
//
// run.py's loopback server adds this script to the runner page it serves; the
// pinned suite on disk is not edited. It uses each suite's own automation
// surface and sends what it reads back to the serving origin:
//
//   POST /__stress/progress   a step began or ended (which iteration, which subtest)
//   POST /__stress/error      an uncaught error, a rejected promise, a console.error
//   POST /__stress/result     the final score and sub-scores
//   GET  /__stress/done?...   the score once more, as a plain GET (an engine
//                             whose POST is broken still reports)
//
// Written in old-fashioned script on purpose: it has to run in an engine
// that may fail the benchmark itself.
(function () {
    'use strict';
    var cfg = window.__stress || {};
    var suite = cfg.suite;
    var trace = !!cfg.trace;
    var errorsSent = 0;

    function xhrPost(kind, body) {
        try {
            var x = new XMLHttpRequest();
            x.open('POST', '/__stress/' + kind, true);
            x.send(body);
        } catch (e) { /* nothing left to try */ }
    }

    function post(kind, obj) {
        var body;
        try { body = JSON.stringify(obj); } catch (e) { body = JSON.stringify({ unserializable: String(e) }); }
        try {
            var p = fetch('/__stress/' + kind, { method: 'POST', body: body });
            if (p && p['catch']) p['catch'](function () { xhrPost(kind, body); });
        } catch (e) {
            xhrPost(kind, body);
        }
    }

    function done(score) {
        try { fetch('/__stress/done?score=' + encodeURIComponent(String(score))); } catch (e) { /* the POST is the record */ }
    }

    function describe(e) {
        if (!e) return { message: String(e) };
        return {
            name: e.name ? String(e.name) : undefined,
            message: String(e.message !== undefined ? e.message : e).slice(0, 600),
            stack: e.stack ? String(e.stack).slice(0, 1500) : undefined
        };
    }

    function sendError(kind, detail) {
        if (errorsSent++ >= 40) return;
        detail.kind = kind;
        post('error', detail);
    }

    window.addEventListener('error', function (ev) {
        var d = describe(ev.error || { message: ev.message });
        d.file = ev.filename ? String(ev.filename).split('?')[0] : undefined;
        d.line = ev.lineno;
        d.col = ev.colno;
        sendError('uncaught', d);
    });
    window.addEventListener('unhandledrejection', function (ev) {
        sendError('unhandledrejection', describe(ev.reason));
    });
    if (window.console && console.error) {
        var consoleError = console.error;
        console.error = function () {
            var parts = [];
            for (var i = 0; i < arguments.length; i++) {
                var a = arguments[i];
                parts.push(a && a.stack ? String(a.message) + '\n' + String(a.stack) : String(a));
            }
            sendError('console.error', { message: parts.join(' ').slice(0, 1500) });
            return consoleError.apply(console, arguments);
        };
    }

    // Wrap obj[name] so `before` runs first and `after` once it returned.
    function wrap(obj, name, before, after) {
        var original = obj[name];
        obj[name] = function () {
            if (before) { try { before.apply(this, arguments); } catch (e) { sendError('reporter', describe(e)); } }
            var out = original ? original.apply(this, arguments) : undefined;
            if (after) { try { after.apply(this, arguments); } catch (e) { sendError('reporter', describe(e)); } }
            return out;
        };
    }

    // Poll for `ready()` (a suite builds its objects in later scripts), then
    // run `then` once. The poll stops before any measured work starts.
    function when(ready, then) {
        var tries = 0;
        var id = setInterval(function () {
            var ok = false;
            try { ok = !!ready(); } catch (e) { ok = false; }
            if (ok) {
                clearInterval(id);
                try { then(); } catch (e) { sendError('reporter', describe(e)); }
            } else if (++tries === 1200) {
                clearInterval(id);
                sendError('reporter', { message: 'the suite never exposed its automation object (60 s)' });
            }
        }, 50);
    }

    // ── Speedometer 3: ?startAutomatically, globalThis.benchmarkClient ──────
    function speedometer() {
        when(function () { return window.benchmarkClient; }, function () {
            var client = window.benchmarkClient;
            var iteration = 0;
            post('progress', { event: 'client-ready' });
            wrap(client, 'willStartFirstIteration', function () { post('progress', { event: 'start' }); });
            wrap(client, 'didRunSuites', null, function () { post('progress', { event: 'iteration-done', iteration: ++iteration }); });
            if (trace) {
                wrap(client, 'willRunTest', function (s, t) { post('progress', { event: 'test', suite: s && s.name, test: t && t.name }); });
            } else {
                var lastSuite = null;
                wrap(client, 'willRunTest', function (s) {
                    if (s && s.name !== lastSuite) { lastSuite = s.name; post('progress', { event: 'suite', suite: s.name, iteration: iteration + 1 }); }
                });
            }
            wrap(client, 'handleError', function (e) { sendError('benchmark', describe(e)); post('result', { score: null, failed: true }); });
            wrap(client, 'didFinishLastIteration', null, function (metrics) {
                var out = { score: null, metrics: {} };
                for (var name in metrics) {
                    var m = metrics[name];
                    if (!m) continue;
                    out.metrics[name] = { mean: m.mean, delta: m.delta, unit: m.unit, values: m.values };
                }
                if (metrics && metrics.Score) out.score = metrics.Score.mean;
                post('result', out);
                done(out.score);
            });
        });
    }

    // ── MotionMark: index.html + benchmarkController.startBenchmark() (what
    //    the Run Benchmark button calls), or developer.html with its query ──
    function motionmark() {
        when(function () { return window.benchmarkRunnerClient && window.benchmarkController; }, function () {
            var client = window.benchmarkRunnerClient;
            post('progress', { event: 'client-ready' });
            wrap(client, 'willStartFirstIteration', function () { post('progress', { event: 'start' }); });
            wrap(client, 'willRunTest', function (s, t) { post('progress', { event: 'test', suite: s && s.name, test: t && t.name }); });
            wrap(client, 'didRunTest', null, function () { post('progress', { event: 'test-done' }); });
            wrap(client, 'didFinishLastIteration', null, function () {
                var dash = client.results;
                var out = { score: dash.score, lower: dash.scoreLowerBound, upper: dash.scoreUpperBound, fps: dash._targetFrameRate || dash._systemFrameRate, tests: {} };
                var iteration = dash.results[0];
                var tests = iteration && iteration[Strings.json.results.tests];
                for (var suiteName in tests) {
                    for (var testName in tests[suiteName]) {
                        var r = tests[suiteName][testName];
                        out.tests[testName] = {
                            score: r[Strings.json.score],
                            lower: r[Strings.json.scoreLowerBound],
                            upper: r[Strings.json.scoreUpperBound]
                        };
                    }
                }
                post('result', out);
                done(out.score);
            });
            if (cfg.start) {
                // The start button is enabled once frame-rate detection ends.
                when(function () { var b = document.getElementById('start-button'); return b && !b.disabled; }, function () {
                    post('progress', { event: 'frame-rate', label: (document.getElementById('frame-rate-label') || {}).textContent });
                    window.benchmarkController.startBenchmark();
                });
            }
        });
    }

    // ── JetStream 2: ?report=true starts the run and POSTs /report itself;
    //    this adds which benchmark is running and which one stopped the run ──
    function jetstream() {
        /* global JetStream */
        when(function () { return typeof JetStream !== 'undefined' && JetStream.benchmarks && JetStream.benchmarks.length; }, function () {
            var scores = {};
            post('progress', { event: 'client-ready', benchmarks: JetStream.benchmarks.length });
            for (var i = 0; i < JetStream.benchmarks.length; i++) {
                (function (b) {
                    wrap(b, 'updateUIBeforeRun', function () { post('progress', { event: 'test', test: b.name }); });
                    wrap(b, 'updateUIAfterRun', null, function () { scores[b.name] = b.score; });
                })(JetStream.benchmarks[i]);
            }
            wrap(JetStream, 'reportError', function (b) {
                sendError('benchmark', { message: 'benchmark failed; JetStream stops the run here', test: b && b.name });
                post('result', { score: null, failed: true, failedTest: b && b.name, tests: scores });
            });
        });
    }

    if (suite === 'speedometer') speedometer();
    else if (suite === 'motionmark') motionmark();
    else if (suite === 'jetstream') jetstream();
    post('progress', { event: 'reporter-loaded', suite: suite });
})();
