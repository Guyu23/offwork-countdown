const RELEASE =
  "https://github.com/Guyu23/offwork-countdown/releases/latest/download/OffWorkCountdown.exe";

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);

    // The only dynamic route: a stable, China-friendly URL for the latest exe.
    if (url.pathname === "/download/OffWorkCountdown.exe") {
      const cache = caches.default;
      const cacheKey = new Request(url.toString(), request);
      let response = await cache.match(cacheKey);

      if (!response) {
        const upstream = await fetch(RELEASE, {
          redirect: "follow",
          cf: { cacheEverything: true, cacheTtl: 3600 },
        });

        if (!upstream.ok) {
          return new Response("Download temporarily unavailable.", {
            status: 502,
            headers: { "content-type": "text/plain; charset=utf-8" },
          });
        }

        const headers = new Headers(upstream.headers);
        headers.set("content-type", "application/octet-stream");
        headers.set(
          "content-disposition",
          'attachment; filename="OffWorkCountdown.exe"'
        );
        headers.set("cache-control", "public, max-age=3600");

        response = new Response(upstream.body, {
          status: 200,
          headers,
        });
        ctx.waitUntil(cache.put(cacheKey, response.clone()));
      }

      return response;
    }

    // Everything else is the static release page in /docs.
    return env.ASSETS.fetch(request);
  },
};
