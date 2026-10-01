import { createRouter } from "@tanstack/react-router";
import { routeTree } from "./routeTree.gen";
import { parseUrlSearch, stringifyUrlSearch } from "../lib/url-search";
export function getRouter() {
  return createRouter({ routeTree, scrollRestoration: true, defaultStaleTime: 0, defaultPreload: false, parseSearch: parseUrlSearch, stringifySearch: stringifyUrlSearch });
}
