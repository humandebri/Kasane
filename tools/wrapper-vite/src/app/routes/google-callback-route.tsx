// どこで: lazy route / 何を: Google callback 完了処理を route module に分離 / なぜ: callback 用依存を初回 dashboard chunk から外すため

import { useEffect, type ReactElement } from "react";
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

export function GoogleCallbackRoute(
  {
    onComplete,
  }: {
    onComplete: (provider: { google: null }) => Promise<void>;
  },
): ReactElement {
  const navigate = useNavigate();

  useEffect(() => {
    let cancelled = false;
    void onComplete({ google: null })
      .catch(() => undefined)
      .finally(() => {
        if (!cancelled) {
          navigate(consumeGoogleReturnToPath() ?? "/", { replace: true });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [navigate, onComplete]);

  return (
    <main className="mx-auto flex min-h-screen w-full max-w-3xl items-center justify-center px-4 py-10">
      <p className="text-sm text-zinc-600">Completing sign-in...</p>
    </main>
  );
}

export const googleCallbackRouteTestHooks = {
  GOOGLE_RETURN_TO_STORAGE_KEY,
  consumeGoogleReturnToPath,
};
