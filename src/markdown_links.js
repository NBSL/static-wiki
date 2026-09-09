// Keep the eval channel alive until the component unmounts.
window.__xpWikiMarkdownLinksCleanup?.();
await new Promise((resolve) => {
    const handler = (event) => {
        if (event.defaultPrevented || event.button !== 0 || event.ctrlKey ||
            event.metaKey || event.shiftKey || event.altKey) return;

        const link = event.target.closest?.(".markdown a[href]");
        if (!link || link.hasAttribute("download") ||
            (link.target && link.target !== "_self")) return;

        const href = link.getAttribute("href");
        // Only wiki page paths belong to the router. Leave media, exports,
        // external URLs, and table-of-contents anchors to the webview.
        if (!/^\/[a-z0-9]+(?:-[a-z0-9]+)*$/.test(href) || href.length > 121) return;

        // Dioxus intercepts anchors at the app root and opens them externally.
        // Handle wiki routes before that listener and stop them reaching it.
        event.preventDefault();
        event.stopPropagation();
        dioxus.send(href.slice(1));
    };
    document.addEventListener("click", handler, true);
    window.__xpWikiMarkdownLinksCleanup = () => {
        document.removeEventListener("click", handler, true);
        resolve();
    };
});
