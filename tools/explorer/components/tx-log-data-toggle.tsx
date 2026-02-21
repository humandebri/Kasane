"use client";

import { useMemo, useState } from "react";
import { Tabs, TabsList, TabsTrigger } from "./ui/tabs";

type Mode = "hex" | "dec";

export function TxLogDataToggle({ dataHex }: { dataHex: string }) {
  const [mode, setMode] = useState<Mode>("dec");
  const decimalText = useMemo(() => hexToDecimalText(dataHex), [dataHex]);

  const handleModeChange = (value: string): void => {
    if (value === "dec" || value === "hex") {
      setMode(value);
    }
  };

  return (
    <div className="relative min-h-6.5">
      <Tabs value={mode} onValueChange={handleModeChange} className="absolute top-0 right-0">
        <TabsList>
          <TabsTrigger value="dec">Dec</TabsTrigger>
          <TabsTrigger value="hex">Hex</TabsTrigger>
        </TabsList>
      </Tabs>
      <div className="pr-24 pt-1.5">{mode === "hex" ? dataHex : decimalText}</div>
    </div>
  );
}

function hexToDecimalText(hex: string): string {
  if (!/^0x[0-9a-fA-F]+$/.test(hex)) {
    return "N/A";
  }
  try {
    return BigInt(hex).toString();
  } catch {
    return "N/A";
  }
}
