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
    // wagmi v2 / RainbowKit transitively pull in connectors (MetaMask
    // SDK, WalletConnect) that reference Node- or React-Native-only
    // modules behind optional `try { require(...) }` guards. Webpack
    // still tries to resolve them at build time; stubbing them to
    // `false` tells webpack to replace each with an empty module and
    // lets the production build succeed. See:
    // https://www.rainbowkit.com/docs/installation#additional-build-tooling-setup
    config.resolve.alias = {
      ...(config.resolve.alias ?? {}),
      '@react-native-async-storage/async-storage': false,
      'pino-pretty': false,
      encoding: false,
      lokijs: false,
    };
    return config;
  },
};

export default nextConfig;
