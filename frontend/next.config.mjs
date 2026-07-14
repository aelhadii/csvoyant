/** @type {import('next').NextConfig} */
const nextConfig = {
  // Emit a standalone server bundle so the Docker runtime image stays small.
  output: "standalone",
};

export default nextConfig;
