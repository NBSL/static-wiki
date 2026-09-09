const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const vm = require('node:vm');
const script = readFileSync('src/markdown_links.js', 'utf8');
const listeners = new Map();
const browserOpens = [];
const navigations = [];
const context = vm.createContext({
    window: {},
    document: {
        addEventListener: (_, handler, capture = false) => listeners.set(handler, capture),
        removeEventListener: (_, handler, capture = false) => {
            if (listeners.get(handler) === capture) listeners.delete(handler);
        },
    },
    dioxus: { send: slug => navigations.push(slug) },
});
function click(href, options = {}) {
    let prevented = false;
    let stopped = false;
    const link = {
        target: options.target || '',
        hasAttribute: () => !!options.download,
        getAttribute: () => href,
    };
    const event = {
        button: 0,
        target: { closest: () => options.outside ? null : link },
        defaultPrevented: false,
        preventDefault: () => { prevented = true; event.defaultPrevented = true; },
        stopPropagation: () => { stopped = true; },
        ...options.event,
    };
    for (const [listener, capture] of listeners) {
        if (capture) listener(event);
    }
    // Dioxus NativeInterpreter.handleClickNavigate runs at the app root,
    // before a document bubble listener, and opens ordinary anchors via IPC.
    if (options.desktop && !stopped) {
        event.preventDefault();
        browserOpens.push(href);
    }
    if (!stopped) {
        for (const [listener, capture] of listeners) {
            if (!capture) listener(event);
        }
    }
    return prevented;
}
vm.runInContext('(async () => {' + script + '})()', context);
assert.equal(click('/test-page'), true);
assert.deepEqual(navigations, ['test-page']);
// Delegation still works after the rendered article changes.
assert.equal(click('/another-page'), true);
for (const href of ['#contents', '/media/image.png', '/exports/latest/index.html',
    'https://example.com/page', '//example.com', '/bad--slug']) {
    assert.equal(click(href), false, href);
}
for (const options of [{ target: '_blank' }, { download: true }, { outside: true },
    { event: { ctrlKey: true } }, { event: { metaKey: true } },
    { event: { shiftKey: true } }, { event: { altKey: true } },
    { event: { button: 1 } }, { event: { defaultPrevented: true } }]) {
    assert.equal(click('/test-page', options), false);
}
// Remounting the hook replaces the old handler instead of navigating twice.
vm.runInContext('(async () => {' + script + '})()', context);
assert.equal(listeners.size, 1);
assert.equal(click('/home'), true);
assert.deepEqual(navigations, ['test-page', 'another-page', 'home']);
// The native root handler must neither swallow the route nor open a browser.
assert.equal(click('/desktop-category-page', { desktop: true }), true);
assert.equal(navigations.at(-1), 'desktop-category-page');
assert.deepEqual(browserOpens, []);
context.window.__xpWikiMarkdownLinksCleanup();
assert.equal(listeners.size, 0);
console.log('Markdown link navigation tests passed');
