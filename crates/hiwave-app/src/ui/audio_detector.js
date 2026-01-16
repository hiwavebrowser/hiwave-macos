// HiWave Audio Detector - monitors media playback and reports to browser
(function() {
    if (window.hiwaveAudioDetector) return;
    window.hiwaveAudioDetector = true;

    let isPlaying = false;
    let checkTimeout = null;

    function checkMediaState() {
        const mediaElements = document.querySelectorAll('video, audio');
        let nowPlaying = false;

        for (const el of mediaElements) {
            if (!el.paused && !el.ended && el.currentTime > 0) {
                nowPlaying = true;
                break;
            }
        }

        if (nowPlaying !== isPlaying) {
            isPlaying = nowPlaying;
            try {
                if (window.ipc && window.ipc.postMessage) {
                    window.ipc.postMessage(JSON.stringify({
                        cmd: 'tab_audio_state_changed',
                        playing: isPlaying
                    }));
                }
            } catch (e) {
                console.error('[HiWave Audio] Failed to send state:', e);
            }
        }
    }

    // Debounced check
    function scheduleCheck() {
        if (checkTimeout) clearTimeout(checkTimeout);
        checkTimeout = setTimeout(checkMediaState, 100);
    }

    // Listen for media events (capture phase to catch all events)
    document.addEventListener('play', scheduleCheck, true);
    document.addEventListener('pause', scheduleCheck, true);
    document.addEventListener('ended', scheduleCheck, true);
    document.addEventListener('volumechange', scheduleCheck, true);

    // Also watch for new media elements via MutationObserver
    const observer = new MutationObserver((mutations) => {
        for (const mutation of mutations) {
            for (const node of mutation.addedNodes) {
                if (node.nodeType === 1) {
                    const mediaElements = node.matches && node.matches('video, audio')
                        ? [node]
                        : (node.querySelectorAll ? node.querySelectorAll('video, audio') : []);
                    if (mediaElements.length > 0) {
                        scheduleCheck();
                        break;
                    }
                }
            }
        }
    });

    observer.observe(document.documentElement, {
        childList: true,
        subtree: true
    });

    // Initial check after a short delay (for page load)
    setTimeout(checkMediaState, 500);

    console.log('[HiWave Audio] Detector initialized');
})();
