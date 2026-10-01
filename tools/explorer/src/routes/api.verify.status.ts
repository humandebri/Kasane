import { createFileRoute } from "@tanstack/react-router";
import { GET } from "../../lib/http/verify-status";

export const Route = createFileRoute("/api/verify/status")({
  server: { handlers: { GET: ({ request }) => GET(request) } },
});
