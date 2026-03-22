/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_IC_HOST?: string;
  readonly VITE_INTERNET_IDENTITY_URL?: string;
  readonly VITE_II_DERIVATION_ORIGIN?: string;
  readonly VITE_KASANE_EVM_CANISTER_ID?: string;
  readonly VITE_WRAP_CANISTER_ID?: string;
  readonly VITE_EVM_WRAP_FACTORY?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
