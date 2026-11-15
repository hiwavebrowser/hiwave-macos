// HiWave Autofill - password form detection and the credential picker.
//
// The app decides everything (see ipc/autofill.rs): it answers a request
// only after a user action in the app, for the page's own origin, and gives
// this helper usernames and opaque ids only. The fill itself is a separate
// script the app runs after the person picks an entry.
(function() {
    'use strict';

    // Installed once per document as a non-configurable, frozen object.
    // A plain (configurable) property already there is replaced.
    const existing = Object.getOwnPropertyDescriptor(window, 'hiwaveAutofill');
    if (existing && !existing.configurable) {
        return;
    }

    // Installed at document start, where the IPC object may not exist yet:
    // keep it if present, else look it up when sending.
    const ipcAtInstall = window.ipc && window.ipc.postMessage ? window.ipc : null;
    let autofillPopup = null;
    let anchorField = null;

    function send(message) {
        const ipc = ipcAtInstall || window.ipc;
        if (ipc && typeof ipc.postMessage === 'function') {
            ipc.postMessage(JSON.stringify(message));
        }
    }

    function handleOutsideClick(e) {
        if (autofillPopup && !autofillPopup.contains(e.target)) {
            hidePopup();
        }
    }

    function hidePopup() {
        if (autofillPopup) {
            autofillPopup.remove();
            autofillPopup = null;
        }
        document.removeEventListener('click', handleOutsideClick, true);
    }

    // Ask the app for the picker list. The app uses the view's own URL;
    // it answers only a request it armed from a user action.
    function trigger() {
        const passwordField = document.querySelector('input[type="password"]:not([disabled])');
        if (!passwordField) {
            return;
        }
        anchorField = passwordField;
        send({ cmd: 'get_credentials_for_autofill' });
    }

    // Called by the app with [{ id, username }].
    function showCredentials(entries) {
        if (!Array.isArray(entries) || entries.length === 0) {
            return;
        }

        hidePopup();

        autofillPopup = document.createElement('div');
        autofillPopup.id = 'hiwave-autofill-popup';
        autofillPopup.style.cssText = `
            position: fixed;
            z-index: 2147483647;
            background: #1e293b;
            border: 1px solid #334155;
            border-radius: 8px;
            box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
            padding: 8px;
            min-width: 250px;
            max-width: 350px;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            font-size: 14px;
        `;

        if (anchorField) {
            const rect = anchorField.getBoundingClientRect();
            autofillPopup.style.top = (rect.bottom + 4) + 'px';
            autofillPopup.style.left = rect.left + 'px';
        } else {
            autofillPopup.style.top = '20%';
            autofillPopup.style.left = '50%';
            autofillPopup.style.transform = 'translateX(-50%)';
        }

        const title = document.createElement('div');
        title.style.cssText = `
            color: #94a3b8;
            font-size: 12px;
            margin-bottom: 8px;
            padding: 0 4px;
        `;
        title.textContent = 'Choose a credential';
        autofillPopup.appendChild(title);

        entries.forEach(entry => {
            const id = String(entry && entry.id);
            const item = document.createElement('div');
            item.style.cssText = `
                padding: 10px 12px;
                border-radius: 6px;
                cursor: pointer;
                color: #f1f5f9;
                transition: background 0.1s;
            `;
            item.onmouseenter = () => item.style.background = '#334155';
            item.onmouseleave = () => item.style.background = 'transparent';
            const name = document.createElement('div');
            name.style.fontWeight = '500';
            name.textContent = String(entry && entry.username);
            const host = document.createElement('div');
            host.style.cssText = 'color: #64748b; font-size: 12px;';
            host.textContent = window.location.hostname;
            item.appendChild(name);
            item.appendChild(host);
            item.onclick = () => {
                hidePopup();
                send({ cmd: 'autofill_pick', id: id });
            };
            autofillPopup.appendChild(item);
        });

        document.body.appendChild(autofillPopup);

        setTimeout(() => {
            document.addEventListener('click', handleOutsideClick, true);
        }, 100);
    }

    const api = Object.freeze({
        trigger: trigger,
        showCredentials: showCredentials,
        hidePopup: hidePopup
    });
    try {
        Object.defineProperty(window, 'hiwaveAutofill', {
            value: api,
            writable: false,
            configurable: false,
            enumerable: false
        });
    } catch (e) {
        console.warn('[HiWave Autofill] not installed');
        return;
    }

    console.log('[HiWave Autofill] Initialized');
})();
