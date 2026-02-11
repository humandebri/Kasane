/// <reference types="node" />

export {};

declare const process: {
  pid: number;
  exit(code?: number): never;
  on(event: string, listener: (...args: unknown[]) => void): void;
  stderr: { write(chunk: string): void };
};

declare namespace NodeJS {
  type Signals = "SIGINT" | "SIGTERM";
}
