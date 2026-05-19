/** @type {import('next').NextConfig} */
const nextConfig = {
  async rewrites() {
    const apiUrl = process.env.NEXT_PUBLIC_DARKPOOL_API_URL;
    if (process.env.NODE_ENV !== 'development' || !apiUrl) {
      return [];
    }
    return [
      {
        source: '/api/v1/:path*',
        destination: `${apiUrl.replace(/\/$/, '')}/v1/:path*`,
      },
    ];
  },
  // The proto-generated TypeScript sources import each other with explicit
  // `.js` extensions (per `import_extension=js` in lib/sdk/buf.gen.yaml).
  // That's the standard pattern for ESM-style TS — tsc + Vitest resolve
  // it through `moduleResolution: bundler` — but Next.js's webpack needs
  // an explicit extensionAlias to follow the same path. Added when F1.11
  // (#78) wired the first route that transitively pulls in the proto
  // chain via lib/mock-store.
  webpack: (config) => {
    config.resolve = config.resolve ?? {};
    config.resolve.extensionAlias = {
      ...(config.resolve.extensionAlias ?? {}),
      '.js': ['.js', '.ts', '.tsx'],
    };
    return config;
  },
};

export default nextConfig;
