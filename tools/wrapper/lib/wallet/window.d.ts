import type { OisyProvider } from "./types";

declare global {
  interface Window {
    oisy?: OisyProvider;
    ic?: {
      oisy?: OisyProvider;
    };
  }
}

export {};
