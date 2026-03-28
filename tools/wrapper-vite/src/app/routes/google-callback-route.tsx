// どこで: lazy route / 何を: Google callback 完了処理を route module に分離 / なぜ: callback 用依存を初回 dashboard chunk から外すため

import { useEffect, useState, type ReactElement } from "react";
import { useNavigate } from "react-router-dom";

const GOOGLE_RETURN_TO_STORAGE_KEY = "wrapper-vite:google-return-to";

function consumeGoogleReturnToPath(): string | null {
  if (typeof globalThis.sessionStorage === "undefined") {
    return null;
  }
  const value = globalThis.sessionStorage.getItem(GOOGLE_RETURN_TO_STORAGE_KEY);
  globalThis.sessionStorage.removeItem(GOOGLE_RETURN_TO_STORAGE_KEY);
  if (value === null || !value.startsWith("/") || value.startsWith("/auth/callback")) {
    return null;
  }
  return value;
}

function formatGoogleCallbackError(error: unknown): string {
  return error instanceof Error ? error.message : "wallet.google_callback_failed";
}

export function GoogleCallbackRoute(
  {
    onComplete,
  }: {
    onComplete: (provider: { google: null }) => Promise<void>;
  },
): ReactElement {
  const navigate = useNavigate();
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let failed = false;
    void onComplete({ google: null })
      .catch((error: unknown) => {
        failed = true;
        if (!cancelled) {
          setError(formatGoogleCallbackError(error));
        }
      })
      .finally(() => {
        if (!cancelled && !failed) {
          navigate(consumeGoogleReturnToPath() ?? "/", { replace: true });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [navigate, onComplete]);

  return (
    <main className="mx-auto flex min-h-screen w-full max-w-3xl items-center justify-center px-4 py-10">
      <div className="space-y-2 text-center">
        <p className="text-sm text-zinc-600">Completing sign-in...</p>
        {error ? <p className="text-sm text-rose-700">{error}</p> : null}
      </div>
    </main>
  );
}

export const googleCallbackRouteTestHooks = {
  GOOGLE_RETURN_TO_STORAGE_KEY,
  consumeGoogleReturnToPath,
  formatGoogleCallbackError,
};
