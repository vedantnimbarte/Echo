import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Pin the workspace root to this folder so font/module resolution doesn't
  // get confused by lockfiles higher up the directory tree.
  turbopack: {
    root: __dirname,
  },
};

export default nextConfig;
