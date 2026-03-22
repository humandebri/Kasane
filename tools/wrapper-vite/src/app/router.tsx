// どこで: ルーター定義 / 何を: dashboard と request route を束ねる / なぜ: request status を URL で再表示できるようにするため

import { Suspense, lazy, type ReactNode, type ReactElement } from "react";
import { handleRedirectCallback } from "@junobuild/core";
import { BrowserRouter, Outlet, Route, Routes } from "react-router-dom";
import type { WrapperDashboardConfigState } from "@/components/dashboard";
import { loadConfig } from "@/lib/config";
import {
  resolveConfiguredDerivationOrigin,
  resolveConfiguredGoogleClientId,
  resolveConfiguredInternetIdentityDomain,
  resolveJunoSatelliteId,
} from "@/lib/config";
import { WalletProvider } from "@/lib/wallet/provider";

function resolveConfig(): WrapperDashboardConfigState {
  try {
    return { cfg: loadConfig(), configError: null };
  } catch (error) {
    const message = error instanceof Error ? error.message : "config.invalid";
    return { cfg: null, configError: message };
  }
}

function AppProviders({ children }: { children: ReactNode }): ReactElement {
  return (
    <WalletProvider
      satelliteId={resolveJunoSatelliteId()}
      googleClientId={resolveConfiguredGoogleClientId()}
      iiDomain={resolveConfiguredInternetIdentityDomain()}
      iiDerivationOrigin={resolveConfiguredDerivationOrigin()}
    >
      {children}
    </WalletProvider>
  );
}

const DashboardRoute = lazy(async () => {
  const module = await import("@/src/app/routes/dashboard-route");
  return { default: module.DashboardRoute };
});

const GoogleCallbackRoute = lazy(async () => {
  const module = await import("@/src/app/routes/google-callback-route");
  return { default: module.GoogleCallbackRoute };
});

function AppShell(): ReactElement {
  return <Outlet />;
}

function DashboardRouteFallback(): ReactElement {
  return (
    <main className="mx-auto flex min-h-screen w-full max-w-7xl flex-col gap-5 px-4 py-7 sm:px-8">
      <section className="rounded-2xl border border-emerald-100 bg-white/85 p-5 shadow-sm backdrop-blur">
        <p className="text-xs font-semibold uppercase tracking-[0.2em] text-emerald-700">Kasane</p>
        <h1 className="mt-1 text-2xl font-semibold tracking-tight text-zinc-900">Wrap / Unwrap Console</h1>
      </section>
      <section className="grid gap-5 lg:grid-cols-[1.95fr_0.8fr_0.8fr] lg:items-start">
        <div className="rounded-2xl border border-emerald-100 bg-white p-6">
          <p className="text-sm text-zinc-600">Loading dashboard...</p>
        </div>
      </section>
    </main>
  );
}

function GoogleCallbackFallback(): ReactElement {
  return (
    <main className="mx-auto flex min-h-screen w-full max-w-3xl items-center justify-center px-4 py-10">
      <p className="text-sm text-zinc-600">Completing sign-in...</p>
    </main>
  );
}

export function AppRouter(): ReactElement {
  return (
    <BrowserRouter>
      <AppProviders>
        <Routes>
          <Route element={<AppShell />}>
            <Route
              path="/"
              element={(
                <Suspense fallback={<DashboardRouteFallback />}>
                  <DashboardRoute configState={resolveConfig()} />
                </Suspense>
              )}
            />
            <Route
              path="/auth/callback"
              element={(
                <Suspense fallback={<GoogleCallbackFallback />}>
                  <GoogleCallbackRoute onComplete={handleRedirectCallback} />
                </Suspense>
              )}
            />
            <Route
              path="/requests/:requestId"
              element={(
                <Suspense fallback={<DashboardRouteFallback />}>
                  <DashboardRoute configState={resolveConfig()} />
                </Suspense>
              )}
            />
          </Route>
        </Routes>
      </AppProviders>
    </BrowserRouter>
  );
}
