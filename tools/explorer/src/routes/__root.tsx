import { createRootRoute, HeadContent, Outlet, Scripts } from "@tanstack/react-router";
import { AppHeader } from "../../components/app-header";
import NotFound from "../../components/not-found";
import css from "../styles.css?url";
export const Route = createRootRoute({
  head: () => ({ meta: [{ charSet: "utf-8" }, { name: "viewport", content: "width=device-width, initial-scale=1" }, { title: "Kasane Explorer" }, { name: "description", content: "Postgres-backed operations explorer for Kasane" }], links: [{ rel: "stylesheet", href: css }, { rel: "icon", href: "/favicon.ico" }] }),
  notFoundComponent: NotFound,
  component: RootLayout,
});
function RootLayout() {
  return <html lang="ja"><head><HeadContent /></head><body>
    <main className="relative mx-auto box-border min-h-dvh w-full max-w-[96rem] space-y-4 px-4 pb-8 pt-5 sm:px-6">
      <AppHeader /><section className="space-y-4"><Outlet /></section>
    </main><Scripts />
  </body></html>;
}
