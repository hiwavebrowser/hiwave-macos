// Navigation API (HTML §7.2 / §7.4), the subset a same-document router uses.
//
// `window.navigation` is a regular attribute of Window, so
// `Object.hasOwn(window, "navigation")` is false and `window.navigation` is
// still truthy. That is enough for a Navigation API polyfill to leave
// `history.state` alone. Chrome has this natively and never loads the polyfill.
//
// Session history is the one document's list in web_history.js. Classic
// `history.state` and `NavigationHistoryEntry.getState` are different values:
// pushState/replaceState update the classic state; getState reads the
// navigation state. pushState does not fire `navigate`.
//
// Gaps:
// - Per-document session history only. There is no joint session history
//   across documents, and no `dispose` event when an entry is dropped.
// - A cross-document `navigate` that is not intercepted hands the URL to the
//   embedder (`requestNavigation`). `committed` and `finished` stay pending.
// - `scroll()` and `focusReset` are accepted and not performed.
// - `formData`, `downloadRequest`, `sourceElement` are null.
//   `hasUAVisualTransition` is false. `info` is copied through.
// - `precommitHandler` is ignored. Commit is immediate, then the intercept
//   handler runs. `redirect()` is not implemented.
// - `navigation.back` / `forward` / `traverseTo` commit in this turn.
//   `history.back` / `forward` / `go` stay queued on a task, then fire the
//   same traverse navigation.
// - `userInitiated` is false. Link clicks go through the same `navigate`
//   event as `location.href`, so `intercept` can cancel the request.
// - `Document.location` is still a configurable prototype accessor.
(function (g) {
    var session = g.__rkSession;
    if (!session || !g.EventTarget || !g.Event) return;
    delete g.__rkSession;

    var NO_STATE = session.noState;
    var hooks = session.hooks;
    var NHE = Symbol('rk.nhe');
    var nhes = new WeakMap();
    var transition = null;

    function named(label, fn) {
        try { Object.defineProperty(fn, 'name', { value: label }); } catch (e) {}
        return fn;
    }
    function domError(message, name) {
        return new DOMException(message, name);
    }
    function hush(p) {
        if (p && typeof p.catch === 'function') p.catch(function () {});
        return p;
    }
    function Result() {
        var rc, rj, fc, fj;
        var committed = new Promise(function (res, rej) { rc = res; rj = rej; });
        var finished = new Promise(function (res, rej) { fc = res; fj = rej; });
        return {
            committed: committed,
            finished: finished,
            key: null,
            id: null,
            commit: function (v) { rc(v); },
            finish: function (v) { fc(v); },
            fail: function (e) { rj(e); fj(e); hush(committed); hush(finished); },
            failFinish: function (e) { fj(e); hush(finished); }
        };
    }
    function early(err) {
        var a = Promise.reject(err), b = Promise.reject(err);
        hush(a);
        hush(b);
        return { committed: a, finished: b };
    }
    function expose(result) {
        return { committed: result.committed, finished: result.finished };
    }
    function readState(entry) {
        if (!entry || entry.navState === NO_STATE) return undefined;
        return structuredClone(entry.navState);
    }
    function uuid() {
        var c = g.crypto;
        if (c && typeof c.randomUUID === 'function') {
            try { return c.randomUUID(); } catch (e) {}
        }
        return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function (ch) {
            var r = Math.random() * 16 | 0;
            return (ch === 'x' ? r : (r & 0x3 | 0x8)).toString(16);
        });
    }

    function NavigationHistoryEntry() { throw new TypeError('Illegal constructor'); }
    function requireNhe(recv) {
        if (!recv || recv[NHE] !== 1) throw new TypeError('Illegal invocation');
    }
    function nheFor(entry) {
        if (!entry) return null;
        var nhe = nhes.get(entry);
        if (nhe) return nhe;
        nhe = Object.create(NavigationHistoryEntry.prototype);
        Object.defineProperty(nhe, NHE, { value: 1 });
        Object.defineProperty(nhe, '_entry', { value: entry });
        nhes.set(entry, nhe);
        return nhe;
    }
    Object.defineProperty(NavigationHistoryEntry.prototype, 'url', {
        get: named('get NavigationHistoryEntry.url', function () { requireNhe(this); return this._entry.url; }),
        enumerable: true, configurable: true
    });
    Object.defineProperty(NavigationHistoryEntry.prototype, 'key', {
        get: named('get NavigationHistoryEntry.key', function () { requireNhe(this); return this._entry.key; }),
        enumerable: true, configurable: true
    });
    Object.defineProperty(NavigationHistoryEntry.prototype, 'id', {
        get: named('get NavigationHistoryEntry.id', function () { requireNhe(this); return this._entry.id; }),
        enumerable: true, configurable: true
    });
    Object.defineProperty(NavigationHistoryEntry.prototype, 'index', {
        get: named('get NavigationHistoryEntry.index', function () {
            requireNhe(this);
            var list = session.entries();
            for (var i = 0; i < list.length; i++) if (list[i] === this._entry) return i;
            return -1;
        }),
        enumerable: true, configurable: true
    });
    Object.defineProperty(NavigationHistoryEntry.prototype, 'sameDocument', {
        get: named('get NavigationHistoryEntry.sameDocument', function () {
            requireNhe(this);
            return this.index !== -1;
        }),
        enumerable: true, configurable: true
    });
    NavigationHistoryEntry.prototype.getState = function () {
        requireNhe(this);
        return readState(this._entry);
    };
    Object.defineProperty(NavigationHistoryEntry.prototype, Symbol.toStringTag, { value: 'NavigationHistoryEntry', configurable: true });

    function NavigationDestination() { throw new TypeError('Illegal constructor'); }
    Object.defineProperty(NavigationDestination.prototype, Symbol.toStringTag, { value: 'NavigationDestination', configurable: true });

    function Navigation() { throw new TypeError('Illegal constructor'); }
    Object.setPrototypeOf(Navigation.prototype, g.EventTarget.prototype);
    var navigation = Object.create(Navigation.prototype);

    function requireNav(recv) {
        if (recv !== navigation) throw new TypeError('Illegal invocation');
    }
    Object.defineProperty(Navigation.prototype, 'currentEntry', {
        get: named('get Navigation.currentEntry', function () { requireNav(this); return nheFor(session.current()); }),
        enumerable: true, configurable: true
    });
    Object.defineProperty(Navigation.prototype, 'canGoBack', {
        get: named('get Navigation.canGoBack', function () { requireNav(this); return session.index() > 0; }),
        enumerable: true, configurable: true
    });
    Object.defineProperty(Navigation.prototype, 'canGoForward', {
        get: named('get Navigation.canGoForward', function () {
            requireNav(this);
            return session.index() < session.entries().length - 1;
        }),
        enumerable: true, configurable: true
    });
    Object.defineProperty(Navigation.prototype, 'transition', {
        get: named('get Navigation.transition', function () { requireNav(this); return transition; }),
        enumerable: true, configurable: true
    });
    Object.defineProperty(Navigation.prototype, Symbol.toStringTag, { value: 'Navigation', configurable: true });
    Navigation.prototype.entries = function () {
        requireNav(this);
        return session.entries().map(nheFor);
    };

    function NavigateEvent(type, init) {
        if (!(this instanceof NavigateEvent)) {
            throw new TypeError("Failed to construct 'NavigateEvent': Please use the 'new' operator.");
        }
        g.Event.call(this, type || 'navigate', init || { cancelable: true });
        init = init || {};
        this.navigationType = init.navigationType || 'push';
        this.destination = init.destination || null;
        this.canIntercept = !!init.canIntercept;
        this.userInitiated = !!init.userInitiated;
        this.hashChange = !!init.hashChange;
        this.formData = null;
        this.downloadRequest = null;
        this.info = init.info !== undefined ? init.info : null;
        this.signal = init.signal || null;
        this.hasUAVisualTransition = false;
        this.sourceElement = null;
        this._handlers = [];
        this._intercepted = false;
    }
    NavigateEvent.prototype = Object.create(g.Event.prototype, {
        constructor: { value: NavigateEvent, writable: true, configurable: true }
    });
    Object.defineProperty(NavigateEvent.prototype, Symbol.toStringTag, { value: 'NavigateEvent', configurable: true });
    NavigateEvent.prototype.intercept = function (options) {
        // Dispatch flag first: a saved event rejects with InvalidStateError
        // after the listener returns, including when canIntercept is false.
        if (this.eventPhase === 0) {
            throw domError("Failed to execute 'intercept' on 'NavigateEvent': intercept() may only be called during dispatch.", 'InvalidStateError');
        }
        if (!this.isTrusted) {
            throw domError("Failed to execute 'intercept' on 'NavigateEvent': The event is not trusted.", 'SecurityError');
        }
        if (!this.canIntercept) {
            throw domError("Failed to execute 'intercept' on 'NavigateEvent': The navigation cannot be intercepted.", 'SecurityError');
        }
        options = options || {};
        if (typeof options.handler === 'function') this._handlers.push(options.handler);
        this._intercepted = true;
    };
    // Accepted so a router can call it. Nothing scrolls.
    NavigateEvent.prototype.scroll = function () {
        if (!this._intercepted) {
            throw domError("Failed to execute 'scroll' on 'NavigateEvent': scroll() requires intercept().", 'InvalidStateError');
        }
    };

    function NavigationCurrentEntryChangeEvent(type, init) {
        if (!(this instanceof NavigationCurrentEntryChangeEvent)) {
            throw new TypeError("Failed to construct 'NavigationCurrentEntryChangeEvent': Please use the 'new' operator.");
        }
        g.Event.call(this, type || 'currententrychange', init);
        init = init || {};
        this.navigationType = init.navigationType !== undefined ? init.navigationType : null;
        this.from = init.from || null;
    }
    NavigationCurrentEntryChangeEvent.prototype = Object.create(g.Event.prototype, {
        constructor: { value: NavigationCurrentEntryChangeEvent, writable: true, configurable: true }
    });
    Object.defineProperty(NavigationCurrentEntryChangeEvent.prototype, Symbol.toStringTag, {
        value: 'NavigationCurrentEntryChangeEvent', configurable: true
    });

    function fireChange(type, fromEntry) {
        var ev = new NavigationCurrentEntryChangeEvent('currententrychange');
        ev.navigationType = type;
        ev.from = nheFor(fromEntry);
        ev.isTrusted = true;
        navigation.dispatchEvent(ev);
    }
    hooks.entryChanged = function (type, from) { fireChange(type, from); };

    function clearTransition(result) {
        if (transition && transition.finished === result.finished) transition = null;
    }
    function success(result, entry) {
        var ev = new g.Event('navigatesuccess');
        ev.isTrusted = true;
        navigation.dispatchEvent(ev);
        var nhe = nheFor(entry);
        result.commit(nhe);
        result.finish(nhe);
        clearTransition(result);
    }
    function failure(result, error, committed) {
        var ev = new g.Event('navigateerror');
        ev.error = error;
        ev.isTrusted = true;
        navigation.dispatchEvent(ev);
        if (committed) result.failFinish(error);
        else result.fail(error);
        clearTransition(result);
        if (result.signal && !result.signal.aborted) {
            try { result.controller.abort(error); } catch (e) {}
        }
    }

    function handlingOf(requested, target) {
        if (requested === 'reload' || requested === 'traverse') return requested;
        if (requested === 'push' || requested === 'replace') return requested;
        if (target.href === session.href()) return 'replace';
        return 'push';
    }
    function canInterceptURL(target) {
        if (session.isFragment(target)) return true;
        try { return session.rewritable(new URL(session.href()), target); } catch (e) { return false; }
    }
    function hashWouldChange(target) {
        try {
            var current = new URL(session.href());
            return session.sameDocument(current, target) && current.hash !== target.hash;
        } catch (e) { return false; }
    }
    function makeDestination(target, navState, key, id, index, sameDocument) {
        var dest = Object.create(NavigationDestination.prototype);
        dest.url = target.href;
        dest.key = key;
        dest.id = id;
        dest.index = index;
        dest.sameDocument = sameDocument;
        dest.getState = function () {
            if (navState === NO_STATE) return undefined;
            return structuredClone(navState);
        };
        return dest;
    }

    function commitPushOrReplace(target, handling, navState, key, id, copyIfMissing) {
        var oldURL = session.href();
        var state = (navState === NO_STATE && copyIfMissing) ? undefined : navState;
        var push = handling === 'push';
        var placed = session.place(target.href, null, state, push, !push, push ? key : session.current().key, id);
        session.hashChanged(oldURL);
        var type = handling === 'reload' ? 'reload' : (push ? 'push' : 'replace');
        fireChange(type, placed.from);
        return placed.entry;
    }

    function runHandlers(event, result, entry) {
        var pending = null, i, ret;
        try {
            for (i = 0; i < event._handlers.length; i++) {
                ret = event._handlers[i].call(undefined);
                if (ret && typeof ret.then === 'function') pending = pending ? pending.then(retThen(ret)) : ret;
            }
        } catch (e) {
            result.commit(nheFor(entry));
            failure(result, e, true);
            return;
        }
        result.commit(nheFor(entry));
        if (!pending) {
            var ev = new g.Event('navigatesuccess');
            ev.isTrusted = true;
            navigation.dispatchEvent(ev);
            result.finish(nheFor(entry));
            clearTransition(result);
            return;
        }
        pending.then(function () {
            var ev = new g.Event('navigatesuccess');
            ev.isTrusted = true;
            navigation.dispatchEvent(ev);
            result.finish(nheFor(entry));
            clearTransition(result);
        }, function (e) { failure(result, e, true); });
    }
    function retThen(ret) {
        return function () { return ret; };
    }

    // Returns 'done', 'cancel', or 'native'. Same-document fragment and
    // same-URL API navigations commit here. Cross-document is left to the caller.
    function runNavigate(target, requested, result, navState, info, source) {
        var handling = handlingOf(requested, target);
        var from = session.current();
        var push = handling === 'push';
        var key = push ? uuid() : from.key;
        var id = uuid();
        result.key = key;
        result.id = id;
        var index = session.index() + (push ? 1 : 0);
        var fragment = session.isFragment(target);
        var sameUrl = target.href === session.href();
        var sameDoc = fragment || (sameUrl && handling !== 'reload');
        var ac = new AbortController();
        result.controller = ac;
        result.signal = ac.signal;
        var event = new NavigateEvent('navigate', {
            cancelable: true,
            navigationType: handling,
            canIntercept: canInterceptURL(target),
            hashChange: hashWouldChange(target),
            info: info,
            signal: ac.signal,
            destination: makeDestination(target, navState, key, id, index, sameDoc)
        });
        event.isTrusted = true;
        transition = { navigationType: handling, from: nheFor(from), finished: result.finished };
        session.setSuppress(true);
        try { navigation.dispatchEvent(event); }
        finally { session.setSuppress(false); }

        if (event.defaultPrevented) {
            failure(result, domError('The navigation was aborted.', 'AbortError'), false);
            return 'cancel';
        }
        if (event._intercepted) {
            var copy = (fragment || handling === 'reload') && navState === NO_STATE;
            var entry = commitPushOrReplace(target, handling === 'reload' ? 'replace' : handling, navState, key, id, copy);
            runHandlers(event, result, entry);
            return 'done';
        }
        if (source !== 'location' && handling !== 'reload' && (fragment || sameUrl)) {
            if (fragment && handling === 'push') session.fragment(target.href, key, id);
            else commitPushOrReplace(target, fragment ? handling : 'replace', navState, key, id, fragment);
            success(result, session.current());
            return 'done';
        }
        if (source === 'location') transition = null;
        return 'native';
    }

    function beginTraverse(entry) {
        var result = Result();
        var list = session.entries();
        var index = -1, i;
        for (i = 0; i < list.length; i++) if (list[i] === entry) index = i;
        if (index < 0) {
            result.fail(domError('Invalid history entry.', 'InvalidStateError'));
            return result;
        }
        var target;
        try { target = new URL(entry.url); } catch (e) {
            result.fail(e);
            return result;
        }
        var from = session.current();
        var ac = new AbortController();
        result.controller = ac;
        result.signal = ac.signal;
        var event = new NavigateEvent('navigate', {
            cancelable: true,
            navigationType: 'traverse',
            canIntercept: true,
            hashChange: hashWouldChange(target),
            signal: ac.signal,
            destination: makeDestination(target, entry.navState, entry.key, entry.id, index, true)
        });
        event.isTrusted = true;
        transition = { navigationType: 'traverse', from: nheFor(from), finished: result.finished };
        session.setSuppress(true);
        try { navigation.dispatchEvent(event); }
        finally { session.setSuppress(false); }
        if (event.defaultPrevented) {
            failure(result, domError('The navigation was aborted.', 'AbortError'), false);
            return result;
        }
        if (event._intercepted) {
            session.commitTraverse(index);
            runHandlers(event, result, session.current());
            return result;
        }
        session.commitTraverse(index);
        success(result, session.current());
        return result;
    }
    hooks.traverse = function (entry) { beginTraverse(entry); };
    hooks.start = function (target, handling) {
        if (!target || (target.protocol !== 'http:' && target.protocol !== 'https:')) return 'native';
        var result = Result();
        return runNavigate(target, handling, result, NO_STATE, undefined, 'location');
    };

    function cloneOption(options) {
        if (!options || !Object.prototype.hasOwnProperty.call(options, 'state')) return { ok: true, state: NO_STATE, info: options && options.info };
        try { return { ok: true, state: structuredClone(options.state), info: options.info }; }
        catch (e) { return { ok: false, error: e }; }
    }

    Navigation.prototype.navigate = function (url, options) {
        requireNav(this);
        options = options || {};
        var history = options.history === undefined ? 'auto' : options.history;
        if (history !== 'auto' && history !== 'push' && history !== 'replace') {
            throw new TypeError("Failed to execute 'navigate' on 'Navigation': The provided value '" +
                history + "' is not a valid enum value of type NavigationHistoryBehavior.");
        }
        var target;
        try { target = new URL(String(url), session.href()); }
        catch (e) { return early(domError("Failed to execute 'navigate' on 'Navigation': Invalid URL.", 'SyntaxError')); }
        if (target.protocol === 'javascript:') {
            return early(domError("Failed to execute 'navigate' on 'Navigation': A javascript: URL is not allowed.", 'NotSupportedError'));
        }
        if (history === 'push' && target.href === session.href()) {
            return early(domError("Failed to execute 'navigate' on 'Navigation': A push of the current URL is not supported.", 'NotSupportedError'));
        }
        var cloned = cloneOption(options);
        if (!cloned.ok) return early(cloned.error);
        var result = Result();
        var decision = runNavigate(target, history, result, cloned.state, cloned.info, 'api');
        if (decision === 'native') {
            var handling = handlingOf(history, target);
            session.request(target, handling === 'replace' || handling === 'reload');
        }
        return expose(result);
    };
    Navigation.prototype.reload = function (options) {
        requireNav(this);
        var cloned = cloneOption(options || {});
        if (!cloned.ok) return early(cloned.error);
        var target = new URL(session.href());
        var result = Result();
        var decision = runNavigate(target, 'reload', result, cloned.state, cloned.info, 'api');
        if (decision === 'native') session.request(target, true);
        return expose(result);
    };
    Navigation.prototype.back = function () {
        requireNav(this);
        var i = session.index();
        if (i <= 0) return early(domError("Failed to execute 'back' on 'Navigation': Cannot go back.", 'InvalidStateError'));
        return expose(beginTraverse(session.entries()[i - 1]));
    };
    Navigation.prototype.forward = function () {
        requireNav(this);
        var list = session.entries();
        var i = session.index();
        if (i >= list.length - 1) return early(domError("Failed to execute 'forward' on 'Navigation': Cannot go forward.", 'InvalidStateError'));
        return expose(beginTraverse(list[i + 1]));
    };
    Navigation.prototype.traverseTo = function (key) {
        requireNav(this);
        var list = session.entries(), found = null, i;
        key = String(key);
        for (i = 0; i < list.length; i++) if (list[i].key === key) found = list[i];
        if (!found) return early(domError("Failed to execute 'traverseTo' on 'Navigation': Invalid key.", 'InvalidStateError'));
        return expose(beginTraverse(found));
    };
    Navigation.prototype.updateCurrentEntry = function (options) {
        requireNav(this);
        if (!options || !Object.prototype.hasOwnProperty.call(options, 'state')) {
            throw new TypeError("Failed to execute 'updateCurrentEntry' on 'Navigation': required member state is undefined.");
        }
        var cloned = structuredClone(options.state);
        var entry = session.current();
        entry.navState = cloned;
        fireChange(null, entry);
    };

    if (g.Window && g.Window.prototype) {
        Object.defineProperty(g.Window.prototype, 'navigation', {
            get: named('get Window.navigation', function () {
                if (this !== g) throw new TypeError('Illegal invocation');
                return navigation;
            }),
            enumerable: true,
            configurable: true
        });
    }
    g.Navigation = Navigation;
    g.NavigateEvent = NavigateEvent;
    g.NavigationHistoryEntry = NavigationHistoryEntry;
    g.NavigationDestination = NavigationDestination;
    g.NavigationCurrentEntryChangeEvent = NavigationCurrentEntryChangeEvent;
})(globalThis);
