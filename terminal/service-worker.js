// ═══════════════════════════════════════════════════════════════════════
// Service Worker for ConPTY Remote Terminal
//
// Caches core static files for offline PWA support.
// Modify CACHE_NAME when updating cached files.
// ═══════════════════════════════════════════════════════════════════════

const CACHE_NAME = 'conpty-terminal-v2';

const CACHE_FILES = [
    '/',
    '/terminal',
    '/style.css',
    '/terminal.js',
    '/manifest.json',
];

// Install — pre-cache core files
self.addEventListener('install', (event) => {
    event.waitUntil(
        caches.open(CACHE_NAME)
            .then((cache) => cache.addAll(CACHE_FILES))
            .then(() => self.skipWaiting())
    );
});

// Activate — clean up old caches
self.addEventListener('activate', (event) => {
    event.waitUntil(
        caches.keys().then((keys) =>
            Promise.all(
                keys.filter((k) => k !== CACHE_NAME).map((k) => caches.delete(k))
            )
        ).then(() => self.clients.claim())
    );
});

// Fetch — cache-first for static files, network-first for API/WebSocket
self.addEventListener('fetch', (event) => {
    const url = new URL(event.request.url);

    // Never cache WebSocket requests
    if (url.protocol === 'ws:' || url.protocol === 'wss:') return;

    // Cache static file requests
    if (CACHE_FILES.some((f) => url.pathname === f || url.pathname === '/style.css' || url.pathname === '/terminal.js' || url.pathname === '/manifest.json')) {
        event.respondWith(
            caches.match(event.request).then((cached) => {
                if (cached) return cached;
                return fetch(event.request).then((response) => {
                    if (response.ok) {
                        const clone = response.clone();
                        caches.open(CACHE_NAME).then((cache) => cache.put(event.request, clone));
                    }
                    return response;
                });
            })
        );
        return;
    }

    // Network-first for everything else (API calls, etc.)
    event.respondWith(
        fetch(event.request).catch(() => caches.match(event.request))
    );
});
