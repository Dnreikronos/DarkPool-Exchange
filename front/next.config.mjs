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
};

export default nextConfig;
