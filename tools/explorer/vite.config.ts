import { defineConfig, loadEnv } from "vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { nitro } from "nitro/vite";
export default defineConfig(({ mode }) => {
  // Server configuration is read at runtime, never exposed through define.
  const env = loadEnv(mode, process.cwd(), "");
  for (const [key, value] of Object.entries(env)) process.env[key] ??= value;
  return {
    server: { port: 3000 },
    plugins: [tailwindcss(), tanstackStart(), nitro({ preset: "node-server" }), react()],
  };
});
