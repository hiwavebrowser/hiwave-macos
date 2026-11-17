add_completion_callback(function (tests, harness) {
    const names = ["PASS", "FAIL", "TIMEOUT", "NOTRUN", "PRECONDITION_FAILED"];
    const statuses = ["OK", "ERROR", "TIMEOUT", "PRECONDITION_FAILED"];
    globalThis.__hiwaveWptResult = encodeURIComponent(JSON.stringify({
        status: statuses[harness.status],
        subtests: tests.map(t => ({name: t.name, status: names[t.status], message: t.message}))
    }));
});
