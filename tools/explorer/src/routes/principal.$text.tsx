import { createFileRoute, notFound, redirect } from "@tanstack/react-router";
import { createServerFn } from "@tanstack/react-start";

const resolvePrincipal = createServerFn({ method: "GET" })
  .validator((text: string) => {
    if (typeof text !== "string") throw new TypeError("invalid principal");
    return text;
  })
  .handler(async ({ data: text }) => {
    const { Principal } = await import("@dfinity/principal");
    const { deriveEvmAddressFromPrincipal } = await import("../../lib/principal");
    try { Principal.fromText(text); } catch { throw notFound(); }
    throw redirect({ href: `/address/${deriveEvmAddressFromPrincipal(text)}?principal=${encodeURIComponent(text)}`, statusCode: 307 });
  });

export const Route = createFileRoute("/principal/$text")({
  loader: ({ params }) => resolvePrincipal({ data: params.text }),
});
