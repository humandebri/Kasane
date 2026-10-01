import { createFileRoute } from "@tanstack/react-router";
import { GET } from "../../lib/http/verify-ops";

export const Route = createFileRoute("/api/ops/verify")({
  server: { handlers: { GET: ({  }) => GET() } },
});
