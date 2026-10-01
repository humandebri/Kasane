import { createFileRoute, redirect } from "@tanstack/react-router";
import { resolveSearchRoute } from "../../lib/search";
import { searchString } from "../../lib/url-search";
export const Route = createFileRoute("/search")({
  validateSearch: (search: Record<string, unknown>) => ({ q: searchString(search.q) }),
  beforeLoad: ({ search }) => { throw redirect({ href: resolveSearchRoute(search.q ?? ""), statusCode: 307 }); },
});
