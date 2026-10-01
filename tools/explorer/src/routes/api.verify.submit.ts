import { createFileRoute } from "@tanstack/react-router";
import { POST } from "../../lib/http/verify-submit";

export const Route = createFileRoute("/api/verify/submit")({
  server: { handlers: { POST: ({ request }) => POST(request) } },
});
