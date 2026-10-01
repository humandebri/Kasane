import { createFileRoute } from "@tanstack/react-router";
import { GET } from "../../lib/http/verified-contract";

export const Route = createFileRoute("/api/contracts/$address/verified")({
  server: { handlers: { GET: ({ request, params }) => GET(request, { params: Promise.resolve(params) }) } },
});
