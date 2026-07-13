/** @type {import('next').NextConfig} */
const nextConfig = {
  output: "standalone",
  basePath: "/dashboard",
  async rewrites() {
    const apiUrl = process.env.API_URL || "http://localhost:8080";
    return [
      {
        source: "/api/:path*",
        destination: `${apiUrl}/api/:path*`,
      },
    ];
  },
};

export default nextConfig;
