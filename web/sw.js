// Lets the installed app start offline. Requests go to the network first, so
// a new version is picked up as soon as it's online; every answer is kept,
// and the copy is used when the network isn't there.
//
// Bump VERSION to make browsers reinstall the worker and start a fresh cache.
const VERSION = 2;

const CACHE = `pdftools-v${VERSION}`;
const APP = [
  "./",
  "index.html",
  "style.css",
  "app.js",
  "engine.js",
  "tools.js",
  "ui.js",
  "wasi.js",
  "worker.js",
  "pdftools-cli.wasm",
  "manifest.webmanifest",
  "icon.png",
  "icon-192.png",
];

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE).then((cache) => cache.addAll(APP.map((url) => new Request(url, { cache: "reload" })))));
  self.skipWaiting();
});

self.addEventListener("activate", (event) =>
  event.waitUntil(
    caches
      .keys()
      .then((names) => Promise.all(names.filter((name) => name.startsWith("pdftools") && name !== CACHE).map((name) => caches.delete(name))))
      .then(() => self.clients.claim()),
  ),
);

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET" || new URL(request.url).origin !== location.origin) return;
  event.respondWith(
    // Revalidate rather than trust the HTTP cache: right after a new version
    // is published, mixing old and new modules would break the page.
    fetch(request, { cache: "no-cache" })
      .then((response) => {
        if (response.ok) {
          const copy = response.clone();
          caches.open(CACHE).then((cache) => cache.put(request, copy));
        }
        return response;
      })
      .catch(async () => (await caches.match(request, { ignoreSearch: true })) ?? Response.error()),
  );
});
