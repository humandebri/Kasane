// どこで: ルーター定義 / 何を: dashboard と request route を束ねる / なぜ: request status を URL で再表示できるようにするため

import { Suspense, lazy, type ReactNode, type ReactElement } from "react";
import { BrowserRouter, Outlet, Route, Routes } from "react-router-dom";
import type { WrapperDashboardConfigState } from "@/components/dashboard";
import { loadConfig } from "@/lib/config";
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
      icHost={resolveConfig().cfg?.icHost ?? "https://icp-api.io"}
      oisyDerivationOrigin={null}
      wrapCanisterId={resolveConfig().cfg?.wrapCanisterId ?? null}
      evmCanisterId={resolveConfig().cfg?.kasaneEvmCanisterId ?? null}
      metaMaskChain={{
        chainId: resolveConfig().cfg?.kasaneChainId ?? 4_801_360n,
        chainName: resolveConfig().cfg?.kasaneChainName ?? "Kasane",
        rpcUrl: resolveConfig().cfg?.kasaneRpcUrl ?? "https://rpc-testnet.kasane.network",
        nativeCurrencySymbol: resolveConfig().cfg?.kasaneNativeCurrencySymbol ?? "ICP",
        blockExplorerUrl: resolveConfig().cfg?.kasaneBlockExplorerUrl ?? null,
      }}
    >
      {children}
    </WalletProvider>
  );
}

const DashboardRoute = lazy(async () => {
  const module = await import("@/src/app/routes/dashboard-route");
  return { default: module.DashboardRoute };
});

function AppShell(): ReactElement {
  return <Outlet />;
}

function DashboardRouteFallback(): ReactElement {
  return (
    <main className="mx-auto flex min-h-screen w-full max-w-7xl items-start justify-center px-4 py-8 sm:px-6">
      <div className="w-full max-w-2xl rounded-[2rem] border border-white/60 bg-white/95 p-5 shadow-xl">
        <p className="text-sm text-zinc-600">Loading dashboard...</p>
      </div>
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
